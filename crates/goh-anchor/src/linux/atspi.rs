//! Read only the focused terminal's character rectangle. Never query or consume tty input.
use crate::{Caret, Reason};
use atspi::{CoordType, Role, State, zbus::{self, proxy::CacheProperties}, proxy::{accessible::AccessibleProxy, bus::BusProxy, text::TextProxy}};
use std::time::Duration;

fn failure(e: impl std::fmt::Display) -> Reason { Reason::Linux(format!("AT-SPI: {e}")) }

pub(super) fn locate(pid: u32) -> Result<(Caret, String, String), Reason> {
    // The entire traversal, including bus discovery, has one deadline. Cancellation drops all proxies.
    futures_lite::future::block_on(futures_lite::future::race(read(pid), async {
        async_io::Timer::after(Duration::from_millis(200)).await;
        Err(failure("caret lookup timed out"))
    }))
}

async fn read(pid: u32) -> Result<(Caret, String, String), Reason> {
    let session = zbus::Connection::session().await.map_err(failure)?;
    let address = BusProxy::new(&session).await.map_err(failure)?.get_address().await.map_err(failure)?;
    let conn = zbus::connection::Builder::address(address.as_str()).map_err(failure)?.build().await.map_err(failure)?;
    let registry = AccessibleProxy::builder(&conn).destination("org.a11y.atspi.Registry").map_err(failure)?
        .cache_properties(CacheProperties::No).build().await.map_err(failure)?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await.map_err(failure)?;
    for app in registry.get_children().await.map_err(failure)?.into_iter().take(128) {
        let Some(name) = app.name_as_str() else { continue; };
        let bus_name = zbus::names::BusName::try_from(name).map_err(failure)?;
        if dbus.get_connection_unix_process_id(bus_name).await.map_err(failure)? != pid { continue; }
        let mut stack = vec![(app, 0)];
        let mut visited = 0;
        let mut visible_terminals = 0;
        let mut found = None;
        while let Some((object, depth)) = stack.pop() {
            visited += 1;
            if visited > 512 { return Err(failure("terminal accessibility tree exceeds traversal limit")); }
            let Some(name) = object.name_as_str() else { continue; };
            let path = object.path_as_str();
            let accessible = AccessibleProxy::builder(&conn).destination(name).map_err(failure)?.path(path).map_err(failure)?
                .cache_properties(CacheProperties::No).build().await.map_err(failure)?;
            let state = accessible.get_state().await.map_err(failure)?;
            if state.contains(State::Defunct) { continue; }
            let role = accessible.get_role().await.map_err(failure)?;
            if role == Role::Terminal && state.contains(State::Showing) {
                visible_terminals += 1;
                if visible_terminals > 1 {
                    return Err(failure("multiple visible terminal panes require a shell-to-pane identity provider"));
                }
            }
            if state.contains(State::Focused) {
                if role != Role::Terminal || !state.contains(State::Showing) {
                    return Err(Reason::NotTerminalInput);
                }
                let text = TextProxy::builder(&conn).destination(name).map_err(failure)?.path(path).map_err(failure)?
                    .cache_properties(CacheProperties::No).build().await.map_err(failure)?;
                let offset = text.caret_offset().await.map_err(failure)?;
                if offset < 0 { return Err(Reason::NoCaretRange); }
                let first = text.get_character_extents(offset, CoordType::Screen).await.map_err(failure)?;
                async_io::Timer::after(Duration::from_millis(8)).await;
                if text.caret_offset().await.map_err(failure)? != offset
                    || text.get_character_extents(offset, CoordType::Screen).await.map_err(failure)? != first
                    || !accessible.get_state().await.map_err(failure)?.contains(State::Focused) {
                    return Err(failure("caret changed during lookup"));
                }
                let (x, y, w, h) = first;
                // A zero/negative or implausibly large rectangle is not a character cell.
                if w <= 0 || w > 200 || h <= 0 || h > 200 { return Err(Reason::EmptyRect); }
                found = Some((Caret { x: x.into(), y: y.into(), height: h.into() }, name.into(), path.into()));
            }
            if depth < 24 {
                for child in accessible.get_children().await.map_err(failure)?.into_iter().take(128).rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
        if let Some(result) = found { return Ok(result); }
    }
    Err(Reason::NoFocusedElement { code: None })
}
