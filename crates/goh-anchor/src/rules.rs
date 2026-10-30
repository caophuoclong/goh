//! Per-app rules, taken from the S-04 probe data (docs/spikes/s04). Pure, so they are tested by
//! replaying the recorded probe samples.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// Apps whose Accessibility caret rectangles were measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    LinuxTerminal,
    ITerm2,
    Terminal,
    /// Its terminal is xterm.js; see [`caret_from_rect`] for what is measured there.
    VsCode,
    /// The Claude desktop app's terminal, also xterm.js.
    Claude,
}

impl App {
    pub fn name(self) -> &'static str {
        match self {
            App::LinuxTerminal => "Linux terminal",
            App::ITerm2 => "iTerm2",
            App::Terminal => "Terminal",
            App::VsCode => "VS Code",
            App::Claude => "Claude",
        }
    }

    /// What `$TERM_PROGRAM` is set to by this app.
    pub fn term_program(self) -> &'static str {
        match self {
            App::LinuxTerminal => "",
            App::ITerm2 => "iTerm.app",
            App::Terminal => "Apple_Terminal",
            App::VsCode => "vscode",
            App::Claude => "claude-desktop",
        }
    }

    /// Length of the text range to ask `AXBoundsForRange` about. iTerm2 answers a zero-length range
    /// with the caret; Terminal.app answers it with an empty rectangle, so it needs the one-character
    /// range (the character under the cursor, or the line break after typed text).
    pub fn caret_range_len(self) -> usize {
        match self {
            App::ITerm2 => 0,
            App::Terminal | App::LinuxTerminal => 1,
            App::VsCode | App::Claude => 0, // unused: xterm.js apps are read from the element's frame
        }
    }

    /// xterm.js terminals: the caret is the frame of the focused hidden input, not a text range's bounds.
    pub fn reads_input_frame(self) -> bool {
        matches!(self, App::VsCode | App::Claude)
    }

    /// Electron apps that build their accessibility tree only when asked (measured: Claude, and VS Code
    /// since 1.144, expose no focused element until `AXManualAccessibility` is set).
    pub fn needs_manual_accessibility(self) -> bool {
        matches!(self, App::VsCode | App::Claude)
    }

    pub fn from_exe_path(path: &str) -> Option<App> {
        if path.ends_with("/iTerm.app/Contents/MacOS/iTerm2") {
            Some(App::ITerm2)
        } else if path.ends_with("/Terminal.app/Contents/MacOS/Terminal") {
            Some(App::Terminal)
        } else if path.ends_with("/Visual Studio Code.app/Contents/MacOS/Code") {
            Some(App::VsCode)
        } else if path.ends_with("/Claude.app/Contents/MacOS/Claude") {
            Some(App::Claude)
        } else {
            None
        }
    }
}

/// Top-left of the caret's cell and the cell height, in global points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caret {
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

/// Why no verified anchor is available. These are diagnostics, shown by `goh-popup --resolve`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    UnsupportedPlatform,
    Linux(String),
    NotTrusted,
    NoFocusedApp { code: Option<i32> },
    UnknownApp(String),
    TermProgramMismatch { expected: &'static str, found: String },
    Multiplexer(&'static str),
    Remote,
    NoFocusedWindow,
    WindowMinimized,
    NoFocusedElement { code: Option<i32> },
    NoCaretRange,
    NoBounds,
    NotTerminalInput,
    CellHeightUnknown,
    EmptyRect,
    NoDisplay,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::UnsupportedPlatform => f.write_str("caret lookup is unsupported on this platform"),
            Reason::Linux(reason) => f.write_str(reason),
            Reason::NotTrusted => f.write_str("this terminal is not allowed to use the Accessibility API (System Settings > Privacy & Security > Accessibility)"),
            Reason::NoFocusedApp { code: Some(c) } => write!(f, "no focused application (Accessibility error {c})"),
            Reason::NoFocusedApp { code: None } => f.write_str("no focused application"),
            Reason::UnknownApp(p) => write!(f, "focused app is not a verified terminal ({p})"),
            Reason::TermProgramMismatch { expected, found } => {
                write!(f, "the shell's TERM_PROGRAM is {found:?} but the focused app expects {expected:?}")
            }
            Reason::Multiplexer(m) => write!(f, "running inside {m}, which was not verified"),
            Reason::Remote => f.write_str("running over ssh: the terminal is on another machine"),
            Reason::NoFocusedWindow => f.write_str("the terminal has no verifiable focused window"),
            Reason::WindowMinimized => f.write_str("the terminal window is minimised"),
            Reason::NoFocusedElement { code: Some(c) } => write!(f, "the app exposes no focused element (Accessibility error {c})"),
            Reason::NoFocusedElement { code: None } => f.write_str("the app exposes no focused element"),
            Reason::NoCaretRange => f.write_str("the focused element has no selected text range"),
            Reason::NoBounds => f.write_str("the focused element returned no bounds for the caret"),
            Reason::NotTerminalInput => f.write_str("the focused element is not the terminal's input (focus is in the editor or another widget)"),
            Reason::CellHeightUnknown => f.write_str("the terminal did not report its cell height"),
            Reason::EmptyRect => f.write_str("the caret rectangle is empty or invalid"),
            Reason::NoDisplay => f.write_str("the caret is not on any known display"),
        }
    }
}

/// Turn the raw rectangle from `AXBoundsForRange` into the caret cell.
///
/// * Only the rectangle's **top-left** and the **cell height** are used, never its width: Terminal.app
///   returns a rectangle as wide as the rest of the line when the caret is at the end of typed text.
/// * VS Code and Claude (xterm.js): `AXBoundsForRange` answers zeros, but the focused element is xterm's hidden input,
///   which sits on the caret cell, so its `AXFrame` *is* the caret (measured: 8 x 16 pt at the cursor).
///   Any larger frame is some other widget that has focus, never the caret.
/// * iTerm2 returns a rectangle two lines tall, starting on the line above, for a caret at column 1
///   after an empty line; the caret is the last line of that rectangle. That needs the cell height
///   (iTerm2 reports it with `OSC 1337 ; ReportCellSize`).
pub fn caret_from_rect(app: App, rect: Rect, cell_height: Option<f64>) -> Result<Caret, Reason> {
    let finite = [rect.x, rect.y, rect.w, rect.h].iter().all(|v| v.is_finite());
    if !finite || rect.h <= 0.0 || rect.h > 200.0 {
        return Err(Reason::EmptyRect);
    }
    match app {
        App::Terminal | App::LinuxTerminal => Ok(Caret { x: rect.x, y: rect.y, height: rect.h }),
        App::VsCode | App::Claude if rect.w > 64.0 || rect.h > 64.0 => Err(Reason::NotTerminalInput),
        App::VsCode | App::Claude => Ok(Caret { x: rect.x, y: rect.y, height: rect.h }),
        App::ITerm2 => {
            let cell = cell_height.filter(|c| c.is_finite() && *c > 0.0).ok_or(Reason::CellHeightUnknown)?;
            if rect.h >= 1.5 * cell {
                Ok(Caret { x: rect.x, y: rect.y + rect.h - cell, height: cell })
            } else {
                Ok(Caret { x: rect.x, y: rect.y, height: rect.h })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Recorded probe runs: (app, JSON). Every sample must map to its grid cell.
    const RUNS: &[(App, &str)] = &[
        (App::ITerm2, include_str!("../../../docs/spikes/s04/iterm2-single-pane-v3.json")),
        (App::Terminal, include_str!("../../../docs/spikes/s04/terminalapp-split-lower-pane-v3.json")),
        (App::Terminal, include_str!("../../../docs/spikes/s04/terminalapp-single-pane-main-screen-v4.json")),
        (App::Terminal, include_str!("../../../docs/spikes/s04/terminalapp-single-pane-second-screen-v4.json")),
    ];

    #[test]
    fn replaying_probe_samples_lands_every_caret_on_its_cell() {
        let mut checked = 0;
        for (app, json) in RUNS {
            let d: Value = serde_json::from_str(json).unwrap();
            let f = |v: &Value, k: &str| v[k].as_f64().unwrap();
            let (cw, ch) = (f(&d["cell_size_pt"], "w"), f(&d["cell_size_pt"], "h"));
            let (x0, y0) = (f(&d["grid_origin_pt"], "x"), f(&d["grid_origin_pt"], "y"));
            for s in d["samples"].as_array().unwrap() {
                let r = &s["rect"];
                let rect = Rect { x: f(r, "x"), y: f(r, "y"), w: f(r, "w"), h: f(r, "h") };
                let caret = caret_from_rect(*app, rect, Some(ch)).unwrap();
                let (col, row) = (s["col"].as_f64().unwrap(), s["row"].as_f64().unwrap());
                let (ex, ey) = (x0 + (col - 1.0) * cw, y0 + (row - 1.0) * ch);
                assert!((caret.x - ex).abs() <= 0.5 && (caret.y - ey).abs() <= 0.5, "{} row {row} col {col}: {caret:?} vs ({ex}, {ey})", app.name());
                assert!((caret.height - ch).abs() <= 0.5, "{} caret height {caret:?}", app.name());
                checked += 1;
            }
        }
        assert_eq!(checked, 4 * 48);
    }

    #[test]
    fn terminal_end_of_line_rect_width_is_ignored() {
        let wide = Rect { x: 71.0, y: 92.0, w: 819.0, h: 15.0 };
        assert_eq!(caret_from_rect(App::Terminal, wide, None).unwrap(), Caret { x: 71.0, y: 92.0, height: 15.0 });
    }

    #[test]
    fn iterm_tall_rect_means_the_last_line_and_needs_the_cell_height() {
        let tall = Rect { x: 5.0, y: 467.0, w: 9.0, h: 38.0 };
        assert_eq!(caret_from_rect(App::ITerm2, tall, Some(19.0)).unwrap(), Caret { x: 5.0, y: 486.0, height: 19.0 });
        assert_eq!(caret_from_rect(App::ITerm2, tall, None), Err(Reason::CellHeightUnknown));
        let normal = Rect { x: 41.0, y: 68.0, w: 0.0, h: 19.0 };
        assert_eq!(caret_from_rect(App::ITerm2, normal, Some(19.0)).unwrap().y, 68.0);
    }

    #[test]
    fn vscode_caret_is_the_frame_of_xterms_input_and_nothing_larger() {
        // Measured with `goh-popup --probe` in VS Code's terminal: focused AXTextField, AXFrame below.
        let frame = Rect { x: 73.0, y: 780.0, w: 8.0, h: 16.0 };
        assert_eq!(caret_from_rect(App::VsCode, frame, None).unwrap(), Caret { x: 73.0, y: 780.0, height: 16.0 });
        // Claude's terminal, measured the same way: AXTextField at x=936 y=154, 8 x 16.
        let claude = Rect { x: 936.0, y: 154.0, w: 8.0, h: 16.0 };
        assert_eq!(caret_from_rect(App::Claude, claude, None).unwrap(), Caret { x: 936.0, y: 154.0, height: 16.0 });
        let editor = Rect { x: 0.0, y: 33.0, w: 900.0, h: 20.0 }; // e.g. a search box
        assert_eq!(caret_from_rect(App::VsCode, editor, None), Err(Reason::NotTerminalInput));
    }

    #[test]
    fn electron_terminals_have_their_accessibility_tree_switched_on() {
        // Measured with `goh-popup --probe`: VS Code 1.144 answers -25212 for the focused element until it is set.
        assert!(App::VsCode.needs_manual_accessibility() && App::Claude.needs_manual_accessibility());
        assert!(!App::ITerm2.needs_manual_accessibility() && !App::Terminal.needs_manual_accessibility());
    }

    #[test]
    fn nonsense_rectangles_are_rejected() {
        for r in [Rect::default(), Rect { x: f64::NAN, y: 0.0, w: 1.0, h: 10.0 }, Rect { x: 0.0, y: 0.0, w: 1.0, h: 5000.0 }] {
            assert_eq!(caret_from_rect(App::Terminal, r, Some(15.0)), Err(Reason::EmptyRect));
        }
    }

    #[test]
    fn apps_are_recognised_by_executable_path_only() {
        assert_eq!(App::from_exe_path("/Applications/iTerm.app/Contents/MacOS/iTerm2"), Some(App::ITerm2));
        assert_eq!(App::from_exe_path("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"), Some(App::Terminal));
        assert_eq!(App::from_exe_path("/Applications/Visual Studio Code.app/Contents/MacOS/Code"), Some(App::VsCode));
        assert_eq!(App::from_exe_path("/Applications/Claude.app/Contents/MacOS/Claude"), Some(App::Claude));
        assert_eq!(App::from_exe_path("/Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper"), None);
        assert_eq!(App::from_exe_path("/Applications/Ghostty.app/Contents/MacOS/ghostty"), None);
        assert_eq!(App::from_exe_path("/tmp/evil/iTerm2"), None);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let r = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        assert!(r.contains(0.0, 0.0) && !r.contains(10.0, 5.0) && !r.contains(5.0, 10.0));
    }
}
