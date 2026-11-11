//! The popup window on Windows: a topmost tool window that is never activated, shown and moved in physical pixels.
//!
//! winit shows a window without activating it only the first time; later it uses `SW_SHOW`, which activates it and
//! would take the keyboard from the terminal. So the popup is shown, moved and hidden here, with flags that never
//! activate it, and egui is only told the size (eframe reads the real visibility back from the window).

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// The popup's window handle, as eframe reports it.
#[derive(Debug, Clone, Copy)]
pub struct Window(isize);

impl Window {
    /// Take the handle from eframe's window and mark the window: no activation by clicks or by being shown, and no
    /// place in the taskbar or Alt+Tab.
    pub fn new(handle: &impl raw_window_handle::HasWindowHandle) -> Option<Window> {
        let raw_window_handle::RawWindowHandle::Win32(h) = handle.window_handle().ok()?.as_raw() else { return None };
        let window = Window(h.hwnd.get());
        // SAFETY: the handle is eframe's live window; changing its extended style has no memory effects.
        unsafe {
            let style = GetWindowLongPtrW(window.hwnd(), GWL_EXSTYLE);
            SetWindowLongPtrW(window.hwnd(), GWL_EXSTYLE, style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
        Some(window)
    }

    fn hwnd(self) -> HWND {
        HWND(self.0 as *mut core::ffi::c_void)
    }

    /// Show at `(x, y)` points of a display whose scale is `scale`, above every other window, without activating.
    pub fn show_at(self, x: f64, y: f64, scale: f64) {
        // SAFETY: SetWindowPos on our own live window.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd(),
                Some(HWND_TOPMOST),
                (x * scale).round() as i32,
                (y * scale).round() as i32,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn hide(self) {
        // SAFETY: ShowWindow on our own live window.
        unsafe {
            let _ = ShowWindow(self.hwnd(), SW_HIDE);
        }
    }
}

/// Windows Terminal's `settings.json`: the Store package's, then Preview's, then an unpackaged install's.
pub fn terminal_settings() -> Option<String> {
    let local = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    [
        local.join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Microsoft\Windows Terminal\settings.json"),
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
}

/// The fonts Windows has installed, as (registry name, file path): the user's own, then the system's.
pub fn installed_fonts() -> Vec<(String, String)> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let mut out = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for (name, file) in font_values(root) {
            // System fonts are listed by file name only; the user's by full path.
            let path = if file.contains('\\') { file } else { format!(r"{windir}\Fonts\{file}") };
            out.push((name, path));
        }
    }
    out
}

fn font_values(root: windows::Win32::System::Registry::HKEY) -> Vec<(String, String)> {
    use windows::core::{w, PWSTR};
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, KEY_READ, REG_SZ};
    let mut out = Vec::new();
    let mut key = HKEY::default();
    // SAFETY: the key is closed after use; name and data buffers outlive each call, and their sizes are passed in.
    unsafe {
        if RegOpenKeyExW(root, w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts"), None, KEY_READ, &mut key) != ERROR_SUCCESS {
            return out;
        }
        for index in 0..4096 {
            let mut name = [0u16; 512];
            let mut name_len = name.len() as u32;
            let mut data = [0u16; 1024];
            let mut data_len = (data.len() * 2) as u32;
            let mut kind = 0u32;
            let status = RegEnumValueW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr().cast()),
                Some(&mut data_len),
            );
            if status == ERROR_MORE_DATA {
                continue; // a name or path longer than the buffers: not a font file the popup can use
            }
            if status != ERROR_SUCCESS {
                break; // ERROR_NO_MORE_ITEMS
            }
            if kind != REG_SZ.0 {
                continue;
            }
            let chars = (data_len as usize / 2).min(data.len());
            let file = String::from_utf16_lossy(&data[..chars]).trim_end_matches('\0').to_string();
            out.push((String::from_utf16_lossy(&name[..name_len as usize]), file));
        }
        let _ = RegCloseKey(key);
    }
    out
}
