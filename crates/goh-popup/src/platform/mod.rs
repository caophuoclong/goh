//! Surface selection is separate from caret verification.
#[cfg(target_os = "linux")]
pub mod x11;
#[cfg(windows)]
pub mod windows;

pub fn focus_safe() -> bool {
    #[cfg(any(target_os = "macos", windows))]
    { true }
    #[cfg(target_os = "linux")]
    { std::env::var_os("DISPLAY").is_some_and(|s| !s.is_empty()) }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    { false }
}
