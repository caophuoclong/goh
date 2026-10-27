//! macOS Accessibility reader. Raw FFI to ApplicationServices/CoreGraphics (no objc needed).

use std::ffi::{c_void, CString};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use crate::gate::check_env;
use crate::retry::{retry, settle_from};
use crate::rules::{caret_from_rect, App, Rect, Reason};
use crate::{Anchor, Options, Outcome};

type CFTypeRef = *const c_void;
type AXError = i32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

const K_AX_VALUE_CGRECT: u32 = 3;
const K_AX_VALUE_CFRANGE: u32 = 4;
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const AX_TIMEOUT_SECS: f32 = 0.25;
const RETRIES: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(60);
/// Re-reads of the caret (25 ms apart) until two agree: the line editor may be mid-redraw.
const SETTLE_READS: usize = 4;
const SETTLE_DELAY: Duration = Duration::from_millis(25);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementGetPid(el: CFTypeRef, pid: *mut i32) -> AXError;
    fn AXUIElementSetMessagingTimeout(el: CFTypeRef, seconds: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(el: CFTypeRef, attr: CFTypeRef, out: *mut CFTypeRef) -> AXError;
    fn AXUIElementCopyParameterizedAttributeValue(el: CFTypeRef, attr: CFTypeRef, param: CFTypeRef, out: *mut CFTypeRef) -> AXError;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    /// Connects a command-line process to the window server (what a GUI app gets from NSApplication).
    fn NSApplicationLoad() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFStringCreateWithCString(alloc: CFTypeRef, c: *const i8, encoding: u32) -> CFTypeRef;
}

/// Owned CoreFoundation object, released on drop.
struct Cf(CFTypeRef);

impl Cf {
    fn new(p: CFTypeRef) -> Option<Cf> {
        (!p.is_null()).then_some(Cf(p))
    }
    fn string(s: &str) -> Cf {
        let c = CString::new(s).expect("no NUL in attribute names");
        // SAFETY: valid C string; a null allocator means the default allocator.
        Cf(unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) })
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: we own one reference to a non-null object.
        unsafe { CFRelease(self.0) }
    }
}

/// Copy an attribute; `Err` carries the AXError code (-25204 is "cannot complete", seen when the
/// first call is made too soon after the process starts).
fn attr(el: &Cf, name: &str) -> Result<Cf, i32> {
    let key = Cf::string(name);
    let mut out: CFTypeRef = std::ptr::null();
    // SAFETY: both refs are live; `out` receives an owned object on success.
    let err = unsafe { AXUIElementCopyAttributeValue(el.0, key.0, &mut out) };
    if err != 0 {
        return Err(err);
    }
    Cf::new(out).ok_or(-1)
}

fn focused_pid() -> Result<(Cf, i32), i32> {
    // SAFETY: plain constructor calls.
    let system = Cf::new(unsafe { AXUIElementCreateSystemWide() }).ok_or(-1)?;
    unsafe { AXUIElementSetMessagingTimeout(system.0, AX_TIMEOUT_SECS) };
    let app = attr(&system, "AXFocusedApplication")?;
    let mut pid = 0i32;
    // SAFETY: `app` is a live AXUIElement.
    match unsafe { AXUIElementGetPid(app.0, &mut pid) } {
        0 => Ok((app, pid)),
        e => Err(e),
    }
}

fn exe_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    // SAFETY: buffer length is passed alongside the pointer.
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

fn selected_range(el: &Cf) -> Option<CFRange> {
    let v = attr(el, "AXSelectedTextRange").ok()?;
    let mut r = CFRange { location: 0, length: 0 };
    // SAFETY: `v` is an AXValue; `r` has the layout of CFRange.
    unsafe { AXValueGetValue(v.0, K_AX_VALUE_CFRANGE, (&mut r as *mut CFRange).cast()) }.then_some(r)
}

fn bounds(el: &Cf, loc: isize, len: isize) -> Option<Rect> {
    let range = CFRange { location: loc, length: len };
    // SAFETY: AXValueCreate copies the CFRange.
    let param = Cf::new(unsafe { AXValueCreate(K_AX_VALUE_CFRANGE, (&range as *const CFRange).cast()) })?;
    let key = Cf::string("AXBoundsForRange");
    let mut out: CFTypeRef = std::ptr::null();
    // SAFETY: live refs; `out` receives an owned AXValue on success.
    if unsafe { AXUIElementCopyParameterizedAttributeValue(el.0, key.0, param.0, &mut out) } != 0 {
        return None;
    }
    let v = Cf::new(out)?;
    let mut r = CGRect::default();
    // SAFETY: `v` is an AXValue holding a CGRect.
    unsafe { AXValueGetValue(v.0, K_AX_VALUE_CGRECT, (&mut r as *mut CGRect).cast()) }
        .then_some(Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height })
}

fn displays() -> Vec<Rect> {
    let mut ids = [0u32; 16];
    let mut n = 0u32;
    // SAFETY: the array length is passed; `n` receives the count.
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut n) } != 0 {
        return vec![];
    }
    ids[..n as usize]
        .iter()
        .map(|&id| {
            // SAFETY: id came from CGGetActiveDisplayList.
            let b = unsafe { CGDisplayBounds(id) };
            Rect { x: b.origin.x, y: b.origin.y, w: b.size.width, h: b.size.height }
        })
        .collect()
}

/// iTerm2's cell height in points via `OSC 1337 ; ReportCellSize` on the controlling terminal,
/// bounded to ~100 ms of waiting (each read waits at most 100 ms; poll/select on /dev/tty are
/// unreliable on macOS). `None` if it cannot be asked or does not answer.
pub fn iterm_cell_height() -> Option<f64> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty").ok()?;
    let fd = tty.as_raw_fd();
    // SAFETY: termios is plain data; restored on every exit path below.
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
        return None;
    }
    let mut raw = saved;
    unsafe { libc::cfmakeraw(&mut raw) };
    raw.c_cc[libc::VMIN] = 0;
    raw.c_cc[libc::VTIME] = 1;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return None;
    }
    let reply = (|| {
        tty.write_all(b"\x1b]1337;ReportCellSize\x07").ok()?;
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut buf = Vec::new();
        while Instant::now() < deadline && buf.len() < 128 {
            let mut chunk = [0u8; 64];
            // SAFETY: reads at most chunk.len() bytes into a live buffer.
            let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
            if n > 0 {
                buf.extend_from_slice(&chunk[..n as usize]);
                if buf.last() == Some(&0x07) || buf.ends_with(b"\x1b\\") {
                    break;
                }
            }
        }
        Some(buf)
    })();
    // SAFETY: restores the attributes read above.
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    let text = String::from_utf8_lossy(&reply?).into_owned();
    let body = text.split("ReportCellSize=").nth(1)?;
    body.split([';', '\x07', '\x1b']).next()?.trim().parse::<f64>().ok()
}

pub fn locate_traced(opts: &Options) -> (Outcome, Vec<String>) {
    let mut trace = Vec::new();
    let outcome = match read(&mut trace, opts) {
        Ok(a) => Outcome::Verified(a),
        Err(r) => Outcome::Unverified(r),
    };
    (outcome, trace)
}

fn note(trace: &mut Vec<String>, step: &str, r: &Result<(Cf, i32), i32>) {
    trace.push(match r {
        Ok(_) => format!("{step}: ok"),
        Err(code) => format!("{step}: Accessibility error {code}"),
    });
}

/// The focused application. A plain command-line process is not connected to the window server, and
/// then the system-wide query fails with "cannot complete" (-25204); GUI processes (such as goh-popup
/// once its window system is up) are connected. Measured in iTerm2: a CoreGraphics call is not enough,
/// `NSApplicationLoad()` is. So try the plain call first (free in the popup) and connect only if it
/// fails, recording each step so a failure says which one was missing.
fn focused_app(trace: &mut Vec<String>) -> Result<(Cf, i32), i32> {
    let r = focused_pid();
    note(trace, "direct", &r);
    if r.is_ok() {
        return r;
    }
    // SAFETY: no arguments; safe to call repeatedly.
    unsafe { NSApplicationLoad() };
    let r = retry(RETRIES, RETRY_DELAY, focused_pid);
    note(trace, "after NSApplicationLoad", &r);
    r
}

fn read(trace: &mut Vec<String>, opts: &Options) -> Result<Anchor, Reason> {
    // SAFETY: no arguments.
    if !unsafe { AXIsProcessTrusted() } {
        return Err(Reason::NotTrusted);
    }
    let (app_el, pid) = focused_app(trace).map_err(|code| Reason::NoFocusedApp { code: Some(code) })?;
    unsafe { AXUIElementSetMessagingTimeout(app_el.0, AX_TIMEOUT_SECS) };
    let path = exe_path(pid).ok_or(Reason::NoFocusedApp { code: None })?;
    let app = App::from_exe_path(&path).ok_or(Reason::UnknownApp(path))?;
    check_env(&|k| std::env::var(k).ok(), app)?;

    let el = retry(RETRIES, RETRY_DELAY, || attr(&app_el, "AXFocusedUIElement"))
        .map_err(|code| Reason::NoFocusedElement { code: Some(code) })?;
    let cell = match app {
        App::ITerm2 => opts.cell_height.or_else(|| if opts.allow_tty_query { iterm_cell_height() } else { None }),
        App::Terminal => None,
    };
    let caret_now = || -> Result<crate::rules::Caret, Reason> {
        let range = selected_range(&el).ok_or(Reason::NoCaretRange)?;
        let rect = bounds(&el, range.location, app.caret_range_len() as isize).ok_or(Reason::NoBounds)?;
        caret_from_rect(app, rect, cell)
    };
    let first = caret_now()?;
    // A shell redraws the line when its menu changes, moving the terminal cursor for a moment; a read
    // taken then puts the popup a row off. Trust the caret only once two reads agree.
    let (caret, settled) = settle_from(first, SETTLE_READS, SETTLE_DELAY, || caret_now().ok());
    trace.push(if settled { "caret: stable".to_string() } else { "caret: still moving after re-reads, using the last".to_string() });
    let display = displays().into_iter().find(|d| d.contains(caret.x, caret.y + caret.height / 2.0)).ok_or(Reason::NoDisplay)?;
    Ok(Anchor { app, caret, display })
}
