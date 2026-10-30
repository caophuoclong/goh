//! Read only the focused terminal's character rectangle. Never query or consume tty input.
use super::Error;
use crate::{Caret, Reason};
use atspi::{CoordType, Role, State, zbus::{self, proxy::CacheProperties}, proxy::{accessible::AccessibleProxy, bus::BusProxy, text::TextProxy}};
use std::{future::Future, time::Duration};

/// A bus or protocol failure: the connection is reopened by the next lookup.
fn failure(e: impl std::fmt::Display) -> Error { Error::Lost(Reason::Linux(format!("AT-SPI: {e}"))) }
fn rejected(why: &str) -> Error { Reason::Linux(format!("AT-SPI: {why}")).into() }

/// Run one lookup, including any bus discovery, under one deadline. Cancellation drops its proxies,
/// and a timed-out bus counts as lost.
pub(super) fn bounded<T>(lookup: impl Future<Output = Result<T, Error>>) -> Result<T, Error> {
    futures_lite::future::block_on(futures_lite::future::race(lookup, async {
        async_io::Timer::after(Duration::from_millis(200)).await;
        Err(failure("caret lookup timed out"))
    }))
}

/// The accessibility bus, kept open across lookups.
pub(super) struct Bus {
    conn: zbus::Connection,
    dbus: zbus::fdo::DBusProxy<'static>,
}

impl Bus {
    pub async fn connect() -> Result<Self, Error> {
        let session = zbus::Connection::session().await.map_err(failure)?;
        let address = BusProxy::new(&session).await.map_err(failure)?.get_address().await.map_err(failure)?;
        let conn = zbus::connection::Builder::address(address.as_str()).map_err(failure)?.build().await.map_err(failure)?;
        let dbus = zbus::fdo::DBusProxy::new(&conn).await.map_err(failure)?;
        Ok(Self { conn, dbus })
    }

    async fn accessible<'a>(&self, name: &'a str, path: &'a str) -> Result<AccessibleProxy<'a>, Error> {
        AccessibleProxy::builder(&self.conn).destination(name).map_err(failure)?.path(path).map_err(failure)?
            .cache_properties(CacheProperties::No).build().await.map_err(failure)
    }

    /// Search the terminal process for its only visible terminal pane, which must be focused.
    pub async fn locate(&self, pid: u32) -> Result<(Caret, String, String), Error> {
        let registry = self.accessible("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root").await?;
        for app in registry.get_children().await.map_err(failure)?.into_iter().take(128) {
            let Some(name) = app.name_as_str() else { continue; };
            // An application may leave the bus between listing and this query; it is not the terminal.
            let Ok(bus_name) = zbus::names::BusName::try_from(name) else { continue; };
            if self.dbus.get_connection_unix_process_id(bus_name).await.ok() != Some(pid) { continue; }
            let mut stack = vec![(app, 0)];
            let mut visited = 0;
            let mut visible_terminals = 0;
            let mut found = None;
            while let Some((object, depth)) = stack.pop() {
                visited += 1;
                if visited > 512 { return Err(rejected("terminal accessibility tree exceeds traversal limit")); }
                let Some(name) = object.name_as_str() else { continue; };
                let path = object.path_as_str();
                let accessible = self.accessible(name, path).await?;
                let state = accessible.get_state().await.map_err(failure)?;
                if state.contains(State::Defunct) { continue; }
                let role = accessible.get_role().await.map_err(failure)?;
                if role == Role::Terminal && state.contains(State::Showing) {
                    visible_terminals += 1;
                    if visible_terminals > 1 {
                        return Err(rejected("multiple visible terminal panes require a shell-to-pane identity provider"));
                    }
                }
                if state.contains(State::Focused) {
                    if role != Role::Terminal || !state.contains(State::Showing) {
                        return Err(Reason::NotTerminalInput.into());
                    }
                    found = Some((self.caret(name, path, &accessible).await?, name.into(), path.into()));
                }
                if depth < 24 {
                    for child in accessible.get_children().await.map_err(failure)?.into_iter().take(128).rev() {
                        stack.push((child, depth + 1));
                    }
                }
            }
            if let Some(result) = found { return Ok(result); }
        }
        Err(Reason::NoFocusedElement { code: None }.into())
    }

    /// Read the caret of a pane that [`Self::locate`] verified, while it is still the focused,
    /// showing terminal. Its shell cannot move to another pane, so the tree is not searched again.
    pub async fn recheck(&self, name: &str, path: &str) -> Result<Caret, Error> {
        let accessible = self.accessible(name, path).await?;
        let state = accessible.get_state().await.map_err(failure)?;
        if state.contains(State::Defunct) || !state.contains(State::Focused) || !state.contains(State::Showing)
            || accessible.get_role().await.map_err(failure)? != Role::Terminal {
            return Err(Reason::NotTerminalInput.into());
        }
        self.caret(name, path, &accessible).await
    }

    async fn caret(&self, name: &str, path: &str, accessible: &AccessibleProxy<'_>) -> Result<Caret, Error> {
        let text = TextProxy::builder(&self.conn).destination(name).map_err(failure)?.path(path).map_err(failure)?
            .cache_properties(CacheProperties::No).build().await.map_err(failure)?;
        let offset = text.caret_offset().await.map_err(failure)?;
        if offset < 0 { return Err(Reason::NoCaretRange.into()); }
        let first = text.get_character_extents(offset, CoordType::Screen).await.map_err(failure)?;
        async_io::Timer::after(Duration::from_millis(8)).await;
        if text.caret_offset().await.map_err(failure)? != offset
            || text.get_character_extents(offset, CoordType::Screen).await.map_err(failure)? != first
            || !accessible.get_state().await.map_err(failure)?.contains(State::Focused) {
            return Err(rejected("caret changed during lookup"));
        }
        let (x, y, w, h) = first;
        // A zero/negative or implausibly large rectangle is not a character cell.
        if w <= 0 || w > 200 || h <= 0 || h > 200 { return Err(Reason::EmptyRect.into()); }
        Ok(Caret { x: x.into(), y: y.into(), height: h.into() })
    }
}
