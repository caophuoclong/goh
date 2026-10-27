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
    ITerm2,
    Terminal,
}

impl App {
    pub fn name(self) -> &'static str {
        match self {
            App::ITerm2 => "iTerm2",
            App::Terminal => "Terminal",
        }
    }

    /// What `$TERM_PROGRAM` is set to by this app.
    pub fn term_program(self) -> &'static str {
        match self {
            App::ITerm2 => "iTerm.app",
            App::Terminal => "Apple_Terminal",
        }
    }

    /// Length of the text range to ask `AXBoundsForRange` about. iTerm2 answers a zero-length range
    /// with the caret; Terminal.app answers it with an empty rectangle, so it needs the one-character
    /// range (the character under the cursor, or the line break after typed text).
    pub fn caret_range_len(self) -> usize {
        match self {
            App::ITerm2 => 0,
            App::Terminal => 1,
        }
    }

    pub fn from_exe_path(path: &str) -> Option<App> {
        if path.ends_with("/iTerm.app/Contents/MacOS/iTerm2") {
            Some(App::ITerm2)
        } else if path.ends_with("/Terminal.app/Contents/MacOS/Terminal") {
            Some(App::Terminal)
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
    NotTrusted,
    NoFocusedApp { code: Option<i32> },
    UnknownApp(String),
    TermProgramMismatch { expected: &'static str, found: String },
    Multiplexer(&'static str),
    Remote,
    NoFocusedElement { code: Option<i32> },
    NoCaretRange,
    NoBounds,
    CellHeightUnknown,
    EmptyRect,
    NoDisplay,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::UnsupportedPlatform => f.write_str("caret lookup is only implemented (and verified) on macOS"),
            Reason::NotTrusted => f.write_str("this terminal is not allowed to use the Accessibility API (System Settings > Privacy & Security > Accessibility)"),
            Reason::NoFocusedApp { code: Some(c) } => write!(f, "no focused application (Accessibility error {c})"),
            Reason::NoFocusedApp { code: None } => f.write_str("no focused application"),
            Reason::UnknownApp(p) => write!(f, "focused app is not a verified terminal ({p})"),
            Reason::TermProgramMismatch { expected, found } => {
                write!(f, "the shell's TERM_PROGRAM is {found:?} but the focused app expects {expected:?}")
            }
            Reason::Multiplexer(m) => write!(f, "running inside {m}, which was not verified"),
            Reason::Remote => f.write_str("running over ssh: the terminal is on another machine"),
            Reason::NoFocusedElement { code: Some(c) } => write!(f, "the app exposes no focused element (Accessibility error {c})"),
            Reason::NoFocusedElement { code: None } => f.write_str("the app exposes no focused element"),
            Reason::NoCaretRange => f.write_str("the focused element has no selected text range"),
            Reason::NoBounds => f.write_str("the focused element returned no bounds for the caret"),
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
/// * iTerm2 returns a rectangle two lines tall, starting on the line above, for a caret at column 1
///   after an empty line; the caret is the last line of that rectangle. That needs the cell height
///   (iTerm2 reports it with `OSC 1337 ; ReportCellSize`).
pub fn caret_from_rect(app: App, rect: Rect, cell_height: Option<f64>) -> Result<Caret, Reason> {
    let finite = [rect.x, rect.y, rect.w, rect.h].iter().all(|v| v.is_finite());
    if !finite || rect.h <= 0.0 || rect.h > 200.0 {
        return Err(Reason::EmptyRect);
    }
    match app {
        App::Terminal => Ok(Caret { x: rect.x, y: rect.y, height: rect.h }),
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
    fn nonsense_rectangles_are_rejected() {
        for r in [Rect::default(), Rect { x: f64::NAN, y: 0.0, w: 1.0, h: 10.0 }, Rect { x: 0.0, y: 0.0, w: 1.0, h: 5000.0 }] {
            assert_eq!(caret_from_rect(App::Terminal, r, Some(15.0)), Err(Reason::EmptyRect));
        }
    }

    #[test]
    fn apps_are_recognised_by_executable_path_only() {
        assert_eq!(App::from_exe_path("/Applications/iTerm.app/Contents/MacOS/iTerm2"), Some(App::ITerm2));
        assert_eq!(App::from_exe_path("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"), Some(App::Terminal));
        assert_eq!(App::from_exe_path("/Applications/Ghostty.app/Contents/MacOS/ghostty"), None);
        assert_eq!(App::from_exe_path("/tmp/evil/iTerm2"), None);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let r = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        assert!(r.contains(0.0, 0.0) && !r.contains(10.0, 5.0) && !r.contains(5.0, 10.0));
    }
}
