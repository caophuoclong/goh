//! X11 window identity and physical screen geometry. Never infer a caret from a window rectangle.
use crate::{Reason, Rect};
use x11rb::{connection::Connection, protocol::{xproto::{ConnectionExt, AtomEnum, MapState}, randr::ConnectionExt as _}};

#[derive(Debug, PartialEq)]
pub(super) struct Window { pub pid: u32, pub bounds: Rect, pub displays: Vec<Rect> }

fn failure(e: impl std::fmt::Display) -> Reason { Reason::Linux(format!("X11: {e}")) }

impl Window {
    pub fn read(window: u32) -> Result<Self, Reason> {
        let (conn, screen) = x11rb::connect(None).map_err(failure)?;
        let root = conn.setup().roots[screen].root;
        let attrs = conn.get_window_attributes(window).map_err(failure)?.reply().map_err(failure)?;
        if attrs.map_state != MapState::VIEWABLE { return Err(Reason::WindowMinimized); }
        // WINDOWID may be a top-level or the terminal's child drawing surface. Focus must be within it.
        let focused = conn.get_input_focus().map_err(failure)?.reply().map_err(failure)?.focus;
        let mut child = focused;
        for _ in 0..32 {
            if child == window { break; }
            if child <= 1 || child == root { return Err(Reason::NoFocusedWindow); }
            child = conn.query_tree(child).map_err(failure)?.reply().map_err(failure)?.parent;
        }
        if child != window { return Err(Reason::NoFocusedWindow); }
        let atom = conn.intern_atom(false, b"_NET_WM_PID").map_err(failure)?.reply().map_err(failure)?.atom;
        let mut parent = window;
        let mut pid = None;
        for _ in 0..32 {
            let prop = conn.get_property(false, parent, atom, AtomEnum::CARDINAL, 0, 1).map_err(failure)?.reply().map_err(failure)?;
            pid = prop.value32().and_then(|mut v| v.next());
            if pid.is_some() || parent == root { break; }
            parent = conn.query_tree(parent).map_err(failure)?.reply().map_err(failure)?.parent;
        }
        let pid = pid.filter(|p| *p > 1).ok_or_else(|| failure("terminal window has no process identity"))?;
        if !is_ancestor(pid) { return Err(failure("focused terminal does not own this shell")); }
        let g = conn.get_geometry(window).map_err(failure)?.reply().map_err(failure)?;
        let xy = conn.translate_coordinates(window, root, 0, 0).map_err(failure)?.reply().map_err(failure)?;
        let bounds = Rect { x: xy.dst_x.into(), y: xy.dst_y.into(), w: g.width.into(), h: g.height.into() };
        // RandR 1.5 returns monitors, not the bounding box of all screens (which may contain gaps).
        let monitors = conn.randr_get_monitors(root, true).map_err(failure)?.reply().map_err(failure)?;
        let displays = monitors.monitors.into_iter().map(|m| Rect { x: m.x.into(), y: m.y.into(), w: m.width.into(), h: m.height.into() }).collect();
        Ok(Self { pid, bounds, displays })
    }
}

fn is_ancestor(wanted: u32) -> bool {
    let mut pid = std::process::id();
    for _ in 0..128 {
        if pid == wanted { return true; }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false; };
        // comm is parenthesized and may itself contain spaces or parentheses.
        let Some(tail) = stat.rsplit_once(") ").map(|(_, tail)| tail) else { return false; };
        let Some(next) = tail.split_whitespace().nth(1).and_then(|s| s.parse().ok()) else { return false; };
        if next <= 1 || next == pid { return false; }
        pid = next;
    }
    false
}
