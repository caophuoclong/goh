//! Windows Terminal through UI Automation.
//!
//! The focused element of the foreground Windows Terminal window is the pane's terminal control, which has the text
//! pattern. With nothing selected, the pattern's selection is an empty range at the cursor; widened to one character
//! it is the cursor's cell, whose rectangle is in physical screen pixels (this process is per-monitor DPI aware).
//! The anchor is returned in points of the caret's display: physical pixels divided by that display's scale, which
//! the anchor carries so the popup can place itself in physical pixels.

use std::cell::RefCell;
use std::time::Duration;

use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, SAFEARRAY};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayGetVartype, SafeArrayUnaccessData};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::System::Variant::{VARENUM, VT_I4, VT_R8};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, UIA_TextPatternId,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic};

use crate::retry::settle_from;
use crate::rules::{caret_from_rect, caret_in_view, App, Caret, Rect, Reason};
use crate::{Anchor, Outcome};

/// Re-reads of the cursor while it moves, as on macOS.
const SETTLE_READS: usize = 4;
const SETTLE_DELAY: Duration = Duration::from_millis(25);

/// The foreground window and the terminal control (its UI Automation runtime id) that hold the caret. One Windows
/// Terminal process runs every window, so the window handle is kept, not the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Focus {
    window: isize,
    element: Vec<i32>,
}

thread_local! {
    /// One UI Automation client per thread: the popup asks from its UI thread on every tick.
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn automation() -> Result<IUIAutomation, Reason> {
    AUTOMATION.with(|slot| {
        if let Some(a) = slot.borrow().as_ref() {
            return Ok(a.clone());
        }
        // SAFETY: plain Win32/COM calls with no pointers of ours. Both may fail harmlessly: DPI awareness is already
        // set when winit created the popup's window, and COM is already initialised (as single-threaded) on its thread.
        let a: IUIAutomation = unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|e| Reason::Windows(format!("UI Automation is unavailable ({e})")))?;
        // A hung terminal must not stall the popup's event loop: give up on it quickly instead.
        if let Ok(a2) = a.cast::<IUIAutomation2>() {
            // SAFETY: setters on a live interface.
            unsafe {
                let _ = a2.SetConnectionTimeout(500);
                let _ = a2.SetTransactionTimeout(200);
            }
        }
        *slot.borrow_mut() = Some(a.clone());
        Ok(a)
    })
}

pub(crate) fn locate() -> (Outcome, Vec<String>) {
    let mut trace = Vec::new();
    let result = read(&mut trace);
    (match result { Ok(a) => Outcome::Verified(a), Err(r) => Outcome::Unverified(r) }, trace)
}

fn read(trace: &mut Vec<String>) -> Result<Anchor, Reason> {
    crate::gate::check_windows_env(&|k| std::env::var(k).ok())?;
    let uia = automation()?;
    // SAFETY: Win32 calls on handles they return; out pointers are to locals that outlive each call.
    let (window, pid, frame) = unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return Err(Reason::NoFocusedWindow);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let path = exe_path(pid).ok_or(Reason::NoFocusedApp { code: None })?;
        trace.push(format!("foreground window {:#x}, process {pid}: {path}", window.0 as isize));
        if App::from_exe_path(&path) != Some(App::WindowsTerminal) {
            return Err(Reason::UnknownApp(path));
        }
        if IsIconic(window).as_bool() {
            return Err(Reason::WindowMinimized);
        }
        let mut frame = RECT::default();
        GetWindowRect(window, &mut frame).map_err(|_| Reason::NoFocusedWindow)?;
        (window, pid, rect(frame))
    };
    // SAFETY: UI Automation calls on live interfaces.
    let element = unsafe {
        let element = uia.GetFocusedElement().map_err(|e| Reason::NoFocusedElement { code: Some(e.code().0) })?;
        if element.CurrentProcessId().ok() != Some(pid as i32) {
            return Err(Reason::NotTerminalInput);
        }
        trace.push(format!("focused element: class {:?}", element.CurrentClassName().map(|s| s.to_string()).unwrap_or_default()));
        element
    };
    // SAFETY: as above.
    let text: IUIAutomationTextPattern = unsafe { element.GetCurrentPatternAs(UIA_TextPatternId) }.map_err(|_| Reason::NotTerminalInput)?;
    let first = cursor_cell(&text)?;
    trace.push(format!("cursor cell: {first:?}"));
    // A shell redraws its line when the menu changes, moving the cursor for a moment: trust it once two reads agree.
    let (cell, settled) = settle_from(first, SETTLE_READS, SETTLE_DELAY, || cursor_cell(&text).ok());
    trace.push(if settled { "caret: stable".to_string() } else { "caret: still moving after re-reads, using the last".to_string() });
    let caret = caret_from_rect(App::WindowsTerminal, cell, None)?;
    // SAFETY: as above.
    let view = unsafe { element.CurrentBoundingRectangle() }.map(rect).map_err(|_| Reason::NoBounds)?;
    if !caret_in_view(caret, view) {
        return Err(Reason::CaretOutOfView);
    }
    if !frame.contains(caret.x, caret.y) {
        return Err(Reason::NoBounds);
    }
    let (display, scale) = monitor(caret).ok_or(Reason::NoDisplay)?;
    let id = runtime_id(&element);
    if id.is_empty() {
        return Err(Reason::NoFocusedElement { code: None });
    }
    trace.push(format!("verified Windows Terminal caret {caret:?} at scale {scale}"));
    let points = Caret { x: caret.x / scale, y: caret.y / scale, height: caret.height / scale };
    let display = Rect { x: display.x / scale, y: display.y / scale, w: display.w / scale, h: display.h / scale };
    let focus = crate::Focus { inner: Focus { window: window.0 as isize, element: id } };
    Ok(Anchor { app: App::WindowsTerminal, caret: points, display, focus, scale })
}

/// The cursor's cell, in physical pixels.
fn cursor_cell(text: &IUIAutomationTextPattern) -> Result<Rect, Reason> {
    // SAFETY: UI Automation calls on live interfaces; the array they return is taken (and freed) by `take_array`.
    unsafe {
        let ranges = text.GetSelection().map_err(|_| Reason::NoCaretRange)?;
        if ranges.Length().unwrap_or(0) < 1 {
            return Err(Reason::NoCaretRange);
        }
        let range = ranges.GetElement(0).map_err(|_| Reason::NoCaretRange)?;
        // A selection made with the mouse is returned instead of the cursor: its start is not the caret.
        let span = range.CompareEndpoints(TextPatternRangeEndpoint_Start, &range, TextPatternRangeEndpoint_End).map_err(|_| Reason::NoCaretRange)?;
        if span != 0 {
            return Err(Reason::Windows("text is selected in the terminal".into()));
        }
        range.ExpandToEnclosingUnit(TextUnit_Character).map_err(|_| Reason::NoBounds)?;
        let rects: Vec<f64> = take_array(range.GetBoundingRectangles().map_err(|_| Reason::NoBounds)?, VT_R8);
        // No rectangle: the cursor's line is not among the visible rows (the pane is scrolled back).
        let [x, y, w, h] = *rects.first_chunk::<4>().ok_or(Reason::CaretOutOfView)?;
        Ok(Rect { x, y, w, h })
    }
}

/// The work area (without the taskbar) of the monitor that holds the caret, and that monitor's scale.
fn monitor(caret: Caret) -> Option<(Rect, f64)> {
    let point = POINT { x: caret.x as i32, y: (caret.y + caret.height / 2.0) as i32 };
    // SAFETY: Win32 calls; `info` and the DPI out values are locals that outlive the calls.
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dx, mut dy) = (0u32, 0u32);
        let scale = match GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) {
            Ok(()) if dx > 0 => f64::from(dx) / 96.0,
            _ => 1.0,
        };
        Some((rect(info.rcWork), scale))
    }
}

fn rect(r: RECT) -> Rect {
    Rect { x: r.left.into(), y: r.top.into(), w: (r.right - r.left).into(), h: (r.bottom - r.top).into() }
}

fn runtime_id(element: &IUIAutomationElement) -> Vec<i32> {
    // SAFETY: the returned array is taken and freed by `take_array`.
    unsafe { element.GetRuntimeId().map(|a| take_array(a, VT_I4)).unwrap_or_default() }
}

/// The values of a one-dimensional SAFEARRAY that UI Automation handed over, which is freed. Empty when the array is
/// null, empty or not of type `kind` (whose Rust type must be `T`).
unsafe fn take_array<T: Copy>(array: *mut SAFEARRAY, kind: VARENUM) -> Vec<T> {
    if array.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: `array` is a live SAFEARRAY we own. Its element type is checked before its data is read as `T`, and the
    // data is read only between AccessData and UnaccessData, within the array's bounds.
    unsafe {
        if SafeArrayGetVartype(array).ok() == Some(kind) {
            if let (Ok(lo), Ok(hi)) = (SafeArrayGetLBound(array, 1), SafeArrayGetUBound(array, 1)) {
                let mut data = std::ptr::null_mut();
                if hi >= lo && SafeArrayAccessData(array, &mut data).is_ok() {
                    out = std::slice::from_raw_parts(data as *const T, (hi - lo + 1) as usize).to_vec();
                    let _ = SafeArrayUnaccessData(array);
                }
            }
        }
        let _ = SafeArrayDestroy(array);
    }
    out
}

fn exe_path(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed after use; the buffer and its length outlive the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}
