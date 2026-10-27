//! Where is the caret on screen? (docs/docs/18, S-04)
//!
//! The only source is the macOS Accessibility API in apps whose behaviour was **measured**
//! (iTerm2, Terminal.app). Everything else is `Unverified`, and an overlay must not be drawn for an
//! unverified anchor. Coordinates are global screen points with a top-left origin, the same space as
//! window frames and winit's outer positions.

mod gate;
mod retry;
mod rules;
#[cfg(target_os = "macos")]
mod ax;

pub use gate::check_env;
pub use retry::{retry, settle_from};
pub use rules::{caret_from_rect, App, Caret, Rect, Reason};

/// A caret position for which the rules were verified, plus the display it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub app: App,
    pub caret: Caret,
    pub display: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Verified(Anchor),
    Unverified(Reason),
}

/// How [`locate_with`] may find what it needs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Options {
    /// The terminal's cell height in points, if the caller already knows it (iTerm2 needs it).
    pub cell_height: Option<f64>,
    /// May the provider ask the terminal for it on the tty? Only safe when nothing else is reading
    /// the terminal: the reply arrives as input, so a shell's line editor would take it as typing.
    pub allow_tty_query: bool,
}

/// Ask the system for the caret of the focused terminal, asking the tty if needed (for use at a
/// prompt or from a command line). Never panics; any doubt is `Unverified`.
pub fn locate() -> Outcome {
    locate_traced().0
}

/// Like [`locate`], plus the steps taken (for `goh-popup --resolve`).
pub fn locate_traced() -> (Outcome, Vec<String>) {
    locate_with(&Options { cell_height: None, allow_tty_query: true })
}

pub fn locate_with(opts: &Options) -> (Outcome, Vec<String>) {
    #[cfg(target_os = "macos")]
    {
        ax::locate_traced(opts)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = opts;
        (Outcome::Unverified(Reason::UnsupportedPlatform), Vec::new())
    }
}

/// iTerm2's cell height in points, asked on the controlling tty (bounded, about 100 ms). Call it only
/// between prompts, never while a line editor is reading the terminal.
pub fn iterm_cell_height() -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        ax::iterm_cell_height()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
