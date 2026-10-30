//! Session hints are not proof of the focused terminal's display backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session { X11, Wayland, Headless }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor { Sway, Hyprland, KWin, Gnome, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub session: Session,
    pub compositor: Compositor,
    /// An X connection may still address only XWayland windows in a Wayland session.
    pub x11_available: bool,
}

pub fn detect(env: &dyn Fn(&str) -> Option<String>) -> Capabilities {
    let present = |k| env(k).is_some_and(|s| !s.is_empty());
    let wayland = present("WAYLAND_DISPLAY") || env("XDG_SESSION_TYPE").as_deref() == Some("wayland");
    let x11_available = present("DISPLAY");
    let desktop = env("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    let compositor = if present("SWAYSOCK") { Compositor::Sway }
        else if present("HYPRLAND_INSTANCE_SIGNATURE") { Compositor::Hyprland }
        else if desktop.split(':').any(|s| s == "kde") { Compositor::KWin }
        else if desktop.split(':').any(|s| s == "gnome") { Compositor::Gnome }
        else { Compositor::Unknown };
    Capabilities { session: if wayland { Session::Wayland } else if x11_available { Session::X11 } else { Session::Headless }, compositor, x11_available }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caps(vars: &[(&str, &str)]) -> Capabilities {
        detect(&|key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string()))
    }
    #[test]
    fn display_does_not_turn_a_wayland_session_into_x11() {
        let c = caps(&[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0"), ("XDG_CURRENT_DESKTOP", "GNOME")]);
        assert_eq!(c.session, Session::Wayland);
        assert!(c.x11_available);
        assert_eq!(c.compositor, Compositor::Gnome);
    }
    #[test]
    fn desktop_names_do_not_determine_the_display_server() {
        for name in ["GNOME", "KDE", "XFCE", "X-Cinnamon"] {
            assert_eq!(caps(&[("DISPLAY", ":1"), ("XDG_CURRENT_DESKTOP", name)]).session, Session::X11);
        }
        assert_eq!(caps(&[]).session, Session::Headless);
        assert_eq!(caps(&[("DISPLAY", "")]).session, Session::Headless);
        assert_eq!(caps(&[("XDG_SESSION_TYPE", "wayland")]).session, Session::Wayland);
    }
}
