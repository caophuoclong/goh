//! Capability-driven Linux anchoring. An X display alone never proves caret access.
pub mod detect;
#[cfg(target_os = "linux")]
mod atspi;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Focus {
    window: u32,
    bus: String,
    path: String,
}

#[cfg(target_os = "linux")]
pub(crate) fn locate() -> (crate::Outcome, Vec<String>) {
    use crate::{Outcome, Reason};
    let env = |k: &str| std::env::var(k).ok();
    let caps = detect::detect(&env);
    let mut trace = vec![format!("Linux capabilities: {caps:?}")];
    let result = (|| {
        for key in ["SSH_CONNECTION", "SSH_TTY"] {
            if env(key).is_some() { return Err(Reason::Remote); }
        }
        for (key, name) in [("TMUX", "tmux"), ("STY", "screen")] {
            if env(key).is_some() { return Err(Reason::Multiplexer(name)); }
        }
        if !caps.x11_available { return Err(Reason::Linux("no X11 display; native Wayland needs a cooperating surface provider".into())); }
        let window = env("WINDOWID").and_then(|s| s.parse::<u32>().ok())
            .filter(|id| *id > 1).ok_or_else(|| Reason::Linux("terminal did not export an X11 WINDOWID; native Wayland terminals use inline".into()))?;
        let x = x11::Window::read(window)?;
        let (caret, bus, path) = atspi::locate(x.pid)?;
        let after = x11::Window::read(window)?;
        if x != after { return Err(Reason::Linux("terminal moved during caret lookup".into())); }
        if !x.bounds.contains(caret.x, caret.y) || !x.bounds.contains(caret.x, caret.y + caret.height - 1.0) {
            return Err(Reason::NoBounds);
        }
        let display = x.displays.into_iter().find(|r| r.contains(caret.x, caret.y)).ok_or(Reason::NoDisplay)?;
        trace.push(format!("verified AT-SPI terminal caret on X11 window {window:#x}"));
        Ok(crate::Anchor { app: crate::App::LinuxTerminal, caret, display,
            focus: crate::Focus { inner: Focus { window, bus, path } } })
    })();
    (match result { Ok(a) => Outcome::Verified(a), Err(r) => Outcome::Unverified(r) }, trace)
}
