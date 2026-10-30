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

/// Why a lookup failed. A `Lost` connection is reopened by the next lookup; any other failure keeps it.
#[cfg(target_os = "linux")]
enum Error {
    Lost(crate::Reason),
    Rejected(crate::Reason),
}

#[cfg(target_os = "linux")]
impl From<crate::Reason> for Error {
    fn from(r: crate::Reason) -> Self { Error::Rejected(r) }
}

#[cfg(target_os = "linux")]
impl From<Error> for crate::Reason {
    fn from(e: Error) -> Self { match e { Error::Lost(r) | Error::Rejected(r) => r } }
}

/// Open connections and the last verified pane. The popup re-checks every tick while a menu is
/// open; reconnecting and searching the accessibility tree each time would stall its event loop.
#[cfg(target_os = "linux")]
struct Session {
    x: Option<x11::Display>,
    bus: Option<atspi::Bus>,
    /// The pane and terminal process from the last verified lookup.
    known: Option<(Focus, u32)>,
}

#[cfg(target_os = "linux")]
static SESSION: std::sync::Mutex<Session> = std::sync::Mutex::new(Session { x: None, bus: None, known: None });

#[cfg(target_os = "linux")]
impl Session {
    fn window(&mut self, window: u32) -> Result<x11::Window, Error> {
        let x = match self.x.take() { Some(x) => x, None => x11::Display::connect()? };
        let read = x.read(window);
        if !matches!(read, Err(Error::Lost(_))) { self.x = Some(x); }
        read
    }

    fn caret(&mut self, window: u32, pid: u32) -> Result<(crate::Caret, String, String), Error> {
        let known = self.known.clone().filter(|(f, p)| f.window == window && *p == pid).map(|(f, _)| f);
        let slot = &mut self.bus;
        atspi::bounded(async move {
            let bus = match slot.take() { Some(bus) => bus, None => atspi::Bus::connect().await? };
            let rechecked = match &known {
                Some(f) => bus.recheck(&f.bus, &f.path).await.ok().map(|c| (c, f.bus.clone(), f.path.clone())),
                None => None,
            };
            // A pane that lost focus or visibility may have been replaced: search again.
            let found = match rechecked { Some(found) => Ok(found), None => bus.locate(pid).await };
            if !matches!(found, Err(Error::Lost(_))) { *slot = Some(bus); }
            found
        })
    }
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
        let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        let x = session.window(window)?;
        let (caret, bus, path) = session.caret(window, x.pid)?;
        let after = session.window(window)?;
        if x != after { return Err(Reason::Linux("terminal moved during caret lookup".into())); }
        if !x.bounds.contains(caret.x, caret.y) || !x.bounds.contains(caret.x, caret.y + caret.height - 1.0) {
            return Err(Reason::NoBounds);
        }
        let display = x.displays.into_iter().find(|r| r.contains(caret.x, caret.y)).ok_or(Reason::NoDisplay)?;
        let focus = Focus { window, bus, path };
        session.known = Some((focus.clone(), x.pid));
        trace.push(format!("verified AT-SPI terminal caret on X11 window {window:#x}"));
        Ok(crate::Anchor { app: crate::App::LinuxTerminal, caret, display, focus: crate::Focus { inner: focus } })
    })();
    (match result { Ok(a) => Outcome::Verified(a), Err(r) => Outcome::Unverified(r) }, trace)
}
