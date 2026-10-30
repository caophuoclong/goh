//! macOS Accessibility reader. Raw FFI to ApplicationServices/CoreGraphics (no objc needed).

use std::ffi::{c_void, CString};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;
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
const LIVE_AX_TIMEOUT_SECS: f32 = 0.05;
// Startup recovery is useful before the first successful lookup. Once connected, failures must
// hide the popup promptly; its next watch tick will try again without blocking on startup retries.
static READY: OnceLock<()> = OnceLock::new();
const RETRIES: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(60);
/// Re-reads of the caret (25 ms apart) until two agree: the line editor may be mid-redraw.
const SETTLE_READS: usize = 4;
const SETTLE_DELAY: Duration = Duration::from_millis(25);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFTypeRef;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementGetPid(el: CFTypeRef, pid: *mut i32) -> AXError;
    fn AXUIElementSetMessagingTimeout(el: CFTypeRef, seconds: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(el: CFTypeRef, attr: CFTypeRef, out: *mut CFTypeRef) -> AXError;
    fn AXUIElementCopyAttributeNames(el: CFTypeRef, out: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(el: CFTypeRef, attr: CFTypeRef, value: CFTypeRef) -> AXError;
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
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> bool;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CFTypeRef) -> bool;
    static kCFBooleanTrue: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFDictionaryCreate(alloc: CFTypeRef, keys: *const CFTypeRef, values: *const CFTypeRef, n: isize, kcb: *const c_void, vcb: *const c_void) -> CFTypeRef;
    fn CFArrayGetCount(a: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFTypeRef, i: isize) -> CFTypeRef;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut u8, size: isize, encoding: u32) -> bool;
    fn CFStringCreateWithCString(alloc: CFTypeRef, c: *const i8, encoding: u32) -> CFTypeRef;
}

/// Owned CoreFoundation object, released on drop.
#[derive(Debug)]
struct Cf(CFTypeRef);

impl Clone for Cf {
    fn clone(&self) -> Self {
        // SAFETY: this is a live owned CF object; the clone owns the additional retain.
        Self(unsafe { CFRetain(self.0) })
    }
}

impl PartialEq for Cf {
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: both CF objects remain alive for the comparison. AX equality identifies the
        // remote element, unlike comparing freshly allocated wrapper addresses or caret positions.
        unsafe { CFEqual(self.0, other.0) }
    }
}
impl Eq for Cf {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Focus {
    pid: i32,
    window: Cf,
    element: Cf,
}

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

fn focused_pid(timeout: f32) -> Result<(Cf, i32), i32> {
    // SAFETY: plain constructor calls.
    let system = Cf::new(unsafe { AXUIElementCreateSystemWide() }).ok_or(-1)?;
    unsafe { AXUIElementSetMessagingTimeout(system.0, timeout) };
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
    let outcome = match read(&mut trace, opts, READY.get().is_none()) {
        Ok(a) => {
            let _ = READY.set(());
            Outcome::Verified(a)
        }
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
fn system_focused_app(trace: &mut Vec<String>, startup: bool, timeout: f32) -> Result<(Cf, i32), i32> {
    let r = focused_pid(timeout);
    note(trace, "direct", &r);
    if r.is_ok() || !startup {
        return r;
    }
    // SAFETY: no arguments; safe to call repeatedly.
    unsafe { NSApplicationLoad() };
    let r = retry(RETRIES, RETRY_DELAY, || focused_pid(timeout));
    note(trace, "after NSApplicationLoad", &r);
    r
}

/// The focused application, or failing that the app hosting this terminal while it is frontmost. The
/// accessibility tree of Claude and VS Code is off until `AXManualAccessibility` is set, and until then the system-wide query
/// answers "no value"; the setting is made only once the app is known, so without this fallback the
/// popup would never start. A hosting app that is not frontmost is not focused, so it stays an error.
fn focused_app(trace: &mut Vec<String>, startup: bool, timeout: f32) -> Result<(Cf, i32), i32> {
    let code = match system_focused_app(trace, startup, timeout) {
        Ok(found) => return Ok(found),
        Err(code) => code,
    };
    let host = hosting_app().ok_or(code)?;
    unsafe { AXUIElementSetMessagingTimeout(host.0 .0, timeout) };
    if bool_attr(&host.0, "AXFrontmost") {
        trace.push("hosting app: frontmost".into());
        Ok(host)
    } else {
        trace.push("hosting app: not frontmost".into());
        Err(code)
    }
}

fn bool_attr(el: &Cf, key: &str) -> bool {
    // SAFETY: the owned attribute is live. Check its type before treating it as a CFBoolean.
    attr(el, key).is_ok_and(|v| unsafe { CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0) })
}

fn read(trace: &mut Vec<String>, opts: &Options, startup: bool) -> Result<Anchor, Reason> {
    // SAFETY: no arguments.
    if !unsafe { AXIsProcessTrusted() } {
        return Err(Reason::NotTrusted);
    }
    let timeout = if startup { AX_TIMEOUT_SECS } else { LIVE_AX_TIMEOUT_SECS };
    let (app_el, pid) = focused_app(trace, startup, timeout).map_err(|code| Reason::NoFocusedApp { code: Some(code) })?;
    unsafe { AXUIElementSetMessagingTimeout(app_el.0, timeout) };
    let path = exe_path(pid).ok_or(Reason::NoFocusedApp { code: None })?;
    let app = App::from_exe_path(&path).ok_or(Reason::UnknownApp(path))?;
    check_env(&|k| std::env::var(k).ok(), app)?;

    // A terminal may still expose a caret after its window is minimised or closed. Reject that
    // state before reading the caret, without waiting for the focused element to reappear.
    let window = attr(&app_el, "AXFocusedWindow").map_err(|_| Reason::NoFocusedWindow)?;
    unsafe { AXUIElementSetMessagingTimeout(window.0, timeout) };
    let minimized = attr(&window, "AXMinimized").map_err(|_| Reason::NoFocusedWindow)?;
    // SAFETY: the owned attribute is live. Check its type before treating it as a CFBoolean.
    if unsafe { CFGetTypeID(minimized.0) != CFBooleanGetTypeID() } {
        return Err(Reason::NoFocusedWindow);
    }
    if unsafe { CFBooleanGetValue(minimized.0) } {
        return Err(Reason::WindowMinimized);
    }

    if app.needs_manual_accessibility() {
        enable_accessibility(&app_el, pid);
    }
    let element = || attr(&app_el, "AXFocusedUIElement");
    let el = if startup { retry(RETRIES, RETRY_DELAY, element) } else { element() }
        .map_err(|code| Reason::NoFocusedElement { code: Some(code) })?;
    unsafe { AXUIElementSetMessagingTimeout(el.0, timeout) };
    let cell = match app {
        App::ITerm2 => opts.cell_height.or_else(|| if opts.allow_tty_query { iterm_cell_height() } else { None }),
        App::LinuxTerminal | App::Terminal | App::VsCode | App::Claude => None,
    };
    let caret_now = || -> Result<crate::rules::Caret, Reason> {
        let rect = if app.reads_input_frame() {
            frame(&el).ok_or(Reason::NoBounds)?
        } else {
            let range = selected_range(&el).ok_or(Reason::NoCaretRange)?;
            bounds(&el, range.location, app.caret_range_len() as isize).ok_or(Reason::NoBounds)?
        };
        caret_from_rect(app, rect, cell)
    };
    let first = caret_now()?;
    // A shell redraws the line when its menu changes, moving the terminal cursor for a moment; a read
    // taken then puts the popup a row off. Trust the caret only once two reads agree.
    let (caret, settled) = settle_from(first, SETTLE_READS, SETTLE_DELAY, || caret_now().ok());
    trace.push(if settled { "caret: stable".to_string() } else { "caret: still moving after re-reads, using the last".to_string() });
    let display = displays().into_iter().find(|d| d.contains(caret.x, caret.y + caret.height / 2.0)).ok_or(Reason::NoDisplay)?;
    let focus = crate::Focus { inner: Focus { pid, window, element: el } };
    Ok(Anchor { app, caret, display, focus })
}

pub fn is_trusted() -> bool {
    // SAFETY: no arguments.
    unsafe { AXIsProcessTrusted() }
}

/// Asks macOS to show its Accessibility prompt, which also adds the responsible app to the list.
pub fn request_trust() -> bool {
    // SAFETY: the dictionary holds one constant key and value and is released after the call.
    unsafe {
        let (keys, values) = ([kAXTrustedCheckOptionPrompt], [kCFBooleanTrue]);
        let Some(opts) = Cf::new(CFDictionaryCreate(std::ptr::null(), keys.as_ptr(), values.as_ptr(), 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks)) else {
            return false;
        };
        AXIsProcessTrustedWithOptions(opts.0)
    }
}

fn cf_string(v: CFTypeRef) -> Option<String> {
    // SAFETY: type-checked before use; the buffer length is passed alongside the pointer.
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
            return None;
        }
        let mut buf = vec![0u8; 512];
        CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, K_CF_STRING_ENCODING_UTF8)
            .then(|| String::from_utf8_lossy(&buf[..buf.iter().position(|&b| b == 0).unwrap_or(buf.len())]).into_owned())
    }
}

fn frame(el: &Cf) -> Option<Rect> {
    let v = attr(el, "AXFrame").ok()?;
    let mut r = CGRect::default();
    // SAFETY: `v` is an AXValue; `r` has the layout of CGRect.
    unsafe { AXValueGetValue(v.0, K_AX_VALUE_CGRECT, (&mut r as *mut CGRect).cast()) }
        .then_some(Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height })
}

fn frame_of(el: &Cf) -> String {
    frame(el).map_or("no AXFrame".to_string(), |r| format!("x={} y={} w={} h={}", r.x, r.y, r.w, r.h))
}

fn describe(el: &Cf, label: &str, out: &mut Vec<String>) {
    let text = |name: &str| attr(el, name).ok().and_then(|v| cf_string(v.0)).unwrap_or_else(|| "-".into());
    out.push(format!("{label}: role={} subrole={} frame: {}", text("AXRole"), text("AXSubrole"), frame_of(el)));
    let mut names: CFTypeRef = std::ptr::null();
    // SAFETY: live element; `names` receives an owned CFArray on success.
    if unsafe { AXUIElementCopyAttributeNames(el.0, &mut names) } == 0 {
        if let Some(arr) = Cf::new(names) {
            let n = unsafe { CFArrayGetCount(arr.0) };
            let list: Vec<String> = (0..n).filter_map(|i| cf_string(unsafe { CFArrayGetValueAtIndex(arr.0, i) })).collect();
            out.push(format!("{label}: attributes: {}", list.join(" ")));
        }
    }
    if let Some(range) = selected_range(el) {
        out.push(format!("{label}: AXSelectedTextRange location={} length={}", range.location, range.length));
        for len in [0, 1] {
            out.push(format!("{label}: AXBoundsForRange({}, {len}) = {:?}", range.location, bounds(el, range.location, len)));
        }
    } else {
        out.push(format!("{label}: no AXSelectedTextRange"));
    }
}

/// Walk the window's element tree (bounded) and report the text inputs in it, for apps that do not name a
/// focused element but may still contain the terminal's input.
fn survey(window: &Cf, out: &mut Vec<String>) {
    const MAX_NODES: usize = 4000;
    let mut queue = std::collections::VecDeque::from([(window.clone(), 0usize)]);
    let (mut seen, mut roles) = (0usize, std::collections::BTreeMap::<String, usize>::new());
    while let Some((el, depth)) = queue.pop_front() {
        seen += 1;
        let role = attr(&el, "AXRole").ok().and_then(|v| cf_string(v.0)).unwrap_or_else(|| "-".into());
        *roles.entry(role.clone()).or_default() += 1;
        if matches!(role.as_str(), "AXTextField" | "AXTextArea") {
            let focused = attr(&el, "AXFocused").ok().is_some_and(|v| unsafe { CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0) });
            out.push(format!("  {role} focused={focused} depth={depth} frame: {}", frame_of(&el)));
        }
        if depth >= 14 || seen >= MAX_NODES {
            continue;
        }
        let Ok(kids) = attr(&el, "AXChildren") else { continue };
        // SAFETY: `kids` is a live CFArray; each element is retained before use.
        unsafe {
            for i in 0..CFArrayGetCount(kids.0) {
                let k = CFArrayGetValueAtIndex(kids.0, i);
                if !k.is_null() {
                    queue.push_back((Cf(CFRetain(k)), depth + 1));
                }
            }
        }
    }
    out.push(format!("survey: {seen} elements{}; roles: {roles:?}", if seen >= MAX_NODES { " (stopped at the limit)" } else { "" }));
}

/// Ask an Electron app to build its accessibility tree, once per process. The first read may still find no
/// focused element while it does (the popup then stays hidden and the next tick tries again).
fn enable_accessibility(app_el: &Cf, pid: i32) {
    static DONE: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());
    let Ok(mut done) = DONE.lock() else { return };
    if !done.contains(&pid) {
        let key = Cf::string("AXManualAccessibility");
        // SAFETY: live refs; kCFBooleanTrue is a constant.
        if unsafe { AXUIElementSetAttributeValue(app_el.0, key.0, kCFBooleanTrue) } == 0 {
            done.push(pid);
        }
    }
}

/// The outermost `.app` among this process's ancestors: the app hosting the terminal. Used when the
/// system-wide "focused application" query fails, so the app can still be asked about directly.
fn hosting_app() -> Option<(Cf, i32)> {
    static PID: OnceLock<Option<i32>> = OnceLock::new();
    let pid = (*PID.get_or_init(|| {
        let mut pid = std::process::id() as i32;
        let mut found = None;
        for _ in 0..32 {
            let out = std::process::Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().ok()?;
            pid = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
            if pid <= 1 {
                break;
            }
            if exe_path(pid).is_some_and(|p| p.contains(".app/Contents/MacOS/")) {
                found = Some(pid);
            }
        }
        found
    }))?;
    // SAFETY: plain constructor; the returned proxy is owned (Create rule).
    Cf::new(unsafe { AXUIElementCreateApplication(pid) }).map(|el| (el, pid))
}

/// What the focused app exposes about its focused element, for deciding whether an app can be verified
/// (docs/spikes/s04). Switches the app's accessibility tree on first (Electron apps such as VS Code
/// build theirs only when asked), then reads again after a moment. Changes nothing else.
pub fn probe() -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: no arguments.
    if !unsafe { AXIsProcessTrusted() } {
        return vec!["not trusted: allow this terminal under Privacy & Security > Accessibility".into()];
    }
    let focused = match system_focused_app(&mut out, true, AX_TIMEOUT_SECS) {
        Ok(f) => f,
        Err(code) => {
            out.push(format!("no focused application (error {code}); asking the app that hosts this terminal instead"));
            match hosting_app() {
                Some(f) => f,
                None => return out,
            }
        }
    };
    let (app_el, pid) = focused;
    out.push(format!("app: {}", exe_path(pid).unwrap_or_default()));
    for round in ["before", "after enabling AXManualAccessibility + AXEnhancedUserInterface"] {
        if round != "before" {
            for name in ["AXManualAccessibility", "AXEnhancedUserInterface"] {
                let key = Cf::string(name);
                // SAFETY: live refs; kCFBooleanTrue is a constant.
                let err = unsafe { AXUIElementSetAttributeValue(app_el.0, key.0, kCFBooleanTrue) };
                out.push(format!("set {name}: {}", if err == 0 { "ok".to_string() } else { format!("error {err}") }));
            }
            std::thread::sleep(Duration::from_millis(1500));
        }
        out.push(format!("-- {round} --"));
        match attr(&app_el, "AXFocusedWindow") {
            Ok(w) => out.push(format!("window frame: {}", frame_of(&w))),
            Err(e) => out.push(format!("no focused window ({e})")),
        }
        match attr(&app_el, "AXFocusedUIElement") {
            Ok(el) => describe(&el, "focused", &mut out),
            Err(e) => {
                out.push(format!("no focused element ({e}); surveying the window's elements"));
                if let Ok(w) = attr(&app_el, "AXFocusedWindow").or_else(|_| attr(&app_el, "AXMainWindow")) {
                    survey(&w, &mut out);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: i32) -> Cf {
        // SAFETY: creating an AX proxy does not access the app or require Accessibility permission.
        Cf::new(unsafe { AXUIElementCreateApplication(pid) }).unwrap()
    }

    #[test]
    fn focus_identity_survives_new_proxies_but_distinguishes_windows_and_panes() {
        let pid = std::process::id() as i32;
        let original = Focus { pid, window: app(pid), element: app(pid) };
        let fresh = Focus { pid, window: app(pid), element: app(pid) };
        assert_eq!(original, fresh, "a fresh AX proxy for the same remote target must still match");
        assert_eq!(original, original.clone(), "retaining a target preserves its identity");
        let another_pane = Focus { pid, window: app(pid), element: app(pid + 1) };
        assert_ne!(original, another_pane, "a different text element in the same window is not the owner");
        let another_window = Focus { pid, window: app(pid + 1), element: app(pid) };
        assert_ne!(original, another_window, "window ownership also matters");
    }
}
