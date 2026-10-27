//! Per-session theme resolution (docs/design-system/theme-resolution.md).
//!
//! A pure function from `(user mode, what the terminal told us)` to design tokens plus a report
//! of which tokens had to be contrast-adjusted. A port of the prototype's `derive()`.

pub mod color;
pub mod osc;

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

pub use color::Rgb;

const TOKENS_JSON: &str = include_str!("../../../docs/design-system/tokens.json");
const MIN_CONTRAST: f64 = 4.5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub surface_base: Rgb,
    pub surface_selected: Rgb,
    pub border_subtle: Rgb,
    pub text_primary: Rgb,
    pub text_muted: Rgb,
    pub accent: Rgb,
    pub text_match: Rgb,
    pub focus_ring: Rgb,
}

impl Tokens {
    fn list(&self) -> [(&'static str, Rgb); 8] {
        [
            ("surface-base", self.surface_base),
            ("surface-selected", self.surface_selected),
            ("border-subtle", self.border_subtle),
            ("text-primary", self.text_primary),
            ("text-muted", self.text_muted),
            ("accent", self.accent),
            ("text-match", self.text_match),
            ("focus-ring", self.focus_ring),
        ]
    }
}

/// The design system's own palettes, read from tokens.json (the single source of values).
fn palette(dark: bool) -> Tokens {
    let doc: Value = serde_json::from_str(TOKENS_JSON).expect("tokens.json is valid JSON");
    let theme = if dark { "dark" } else { "light" };
    let tokens = doc["color"]["tokens"].as_array().expect("color.tokens");
    let get = |name: &str| -> Rgb {
        let t = tokens.iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("token {name}"));
        let v = t["value"][theme].as_str().unwrap_or_else(|| panic!("token {name} has no {theme} value"));
        match v.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
            Some(alias) => {
                let a = tokens.iter().find(|t| t["name"] == alias).unwrap_or_else(|| panic!("alias {alias}"));
                Rgb::from_hex(a["value"][theme].as_str().unwrap_or("")).unwrap_or_else(|| panic!("alias {alias}"))
            }
            None => Rgb::from_hex(v).unwrap_or_else(|| panic!("token {name}")),
        }
    };
    Tokens {
        surface_base: get("surface-base"),
        surface_selected: get("surface-selected"),
        border_subtle: get("border-subtle"),
        text_primary: get("text-primary"),
        text_muted: get("text-muted"),
        accent: get("accent"),
        text_match: get("text-match"),
        focus_ring: get("focus-ring"),
    }
}

/// Where the resolved colours came from, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// User chose light, dark or custom colours.
    Override,
    /// Terminal answered OSC 10 and 11.
    Verified,
    /// Only one of foreground/background is known; the other is derived.
    Approximate,
    /// `COLORFGBG` told us whether the background is dark or light.
    EnvHint,
    /// Nothing known: the design system's dark theme.
    Fallback,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Override => "override",
            Source::Verified => "verified",
            Source::Approximate => "approximate",
            Source::EnvHint => "env-hint",
            Source::Fallback => "fallback",
        }
    }
}

/// Outcome of the OSC colour query (docs/05 `osc_color_query`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscStatus {
    Verified,
    Unavailable,
    Unknown,
}

impl OscStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OscStatus::Verified => "verified",
            OscStatus::Unavailable => "unavailable",
            OscStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Terminal {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    /// Background colour index from `COLORFGBG`.
    pub colorfgbg_bg: Option<u8>,
    /// Whether an OSC query was actually sent.
    pub probed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Auto,
    Light,
    Dark,
    Custom { fg: Rgb, bg: Rgb, accent: Option<Rgb> },
}

/// `theme` section of config.json.
#[derive(Debug, Deserialize)]
pub struct ThemeConfig {
    pub mode: String,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub accent: Option<String>,
}

impl ThemeConfig {
    /// An unusable `custom` block (missing or invalid colours) falls back to `auto`.
    pub fn mode(&self) -> Mode {
        match self.mode.as_str() {
            "light" => Mode::Light,
            "dark" => Mode::Dark,
            "custom" => {
                let c = |v: &Option<String>| v.as_deref().and_then(Rgb::from_hex);
                match (c(&self.foreground), c(&self.background)) {
                    (Some(fg), Some(bg)) => Mode::Custom { fg, bg, accent: c(&self.accent) },
                    _ => Mode::Auto,
                }
            }
            _ => Mode::Auto,
        }
    }
}

/// `COLORFGBG` is `fg;bg` (sometimes `fg;default;bg`); returns the background index.
/// `$GOH_CONFIG`, else `~/.config/goh/config.json`.
pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("GOH_CONFIG")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/goh/config.json")))
}

/// The config file as JSON; a missing or unparsable file is `None` (every setting then has its default).
pub fn read_config() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(config_path()?).ok()?).ok()
}

pub fn parse_colorfgbg(s: &str) -> Option<u8> {
    s.rsplit(';').next()?.trim().parse().ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adjustment {
    pub token: &'static str,
    pub from: Rgb,
    pub to: Rgb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub tokens: Tokens,
    pub source: Source,
    pub osc: OscStatus,
    /// Tokens whose terminal-supplied colour failed contrast and was moved.
    pub adjusted: Vec<Adjustment>,
}

/// Move `c` towards white (dark surfaces) or black (light surfaces) in 2% steps until it meets
/// `min` against every surface; unchanged if it already does.
fn ensure(c: Rgb, bgs: &[Rgb], min: f64) -> Rgb {
    let avg = bgs.iter().map(|b| b.luminance()).sum::<f64>() / bgs.len() as f64;
    let target = if avg < 0.4 { Rgb([255, 255, 255]) } else { Rgb([0, 0, 0]) };
    let mut t = 0.0;
    while t <= 1.0001 {
        let m = c.mix(target, t);
        if bgs.iter().all(|&b| m.contrast(b) >= min) {
            return if t == 0.0 { c } else { m };
        }
        t += 0.02;
    }
    target
}

/// Derive every token from a background, a foreground and (only if verified) an accent.
/// Without an accent, matches fall back to the primary text, drawn bold and underlined.
fn derive(bg: Rgb, fg: Rgb, accent: Option<Rgb>) -> (Tokens, Vec<Adjustment>) {
    let mut adj: Vec<Adjustment> = Vec::new();
    let mut primary = ensure(fg, &[bg], MIN_CONTRAST);
    let sel = bg.mix(accent.unwrap_or(primary), 0.18);
    let primary2 = ensure(primary, &[bg, sel], MIN_CONTRAST);
    primary = primary2;
    if primary != fg {
        adj.push(Adjustment { token: "text-primary", from: fg, to: primary });
    }
    let muted0 = primary.mix(bg, 0.4);
    let muted = ensure(muted0, &[bg, sel], MIN_CONTRAST);
    if muted != muted0 {
        adj.push(Adjustment { token: "text-muted", from: muted0, to: muted });
    }
    let accent_out = match accent {
        Some(a) => {
            let e = ensure(a, &[bg, sel], MIN_CONTRAST);
            if e != a {
                adj.push(Adjustment { token: "accent", from: a, to: e });
            }
            e
        }
        None => primary,
    };
    let tokens = Tokens {
        surface_base: bg,
        surface_selected: sel,
        border_subtle: bg.mix(primary, 0.18),
        text_primary: primary,
        text_muted: muted,
        accent: accent_out,
        text_match: accent_out,
        focus_ring: accent_out,
    };
    (tokens, adj)
}

pub fn resolve(mode: Mode, term: &Terminal) -> Resolved {
    let osc = match (term.probed, term.fg.is_some() && term.bg.is_some()) {
        (_, true) => OscStatus::Verified,
        (true, false) if term.fg.is_none() && term.bg.is_none() => OscStatus::Unavailable,
        _ => OscStatus::Unknown,
    };
    let designed = |dark: bool, source: Source| Resolved { tokens: palette(dark), source, osc, adjusted: vec![] };
    let derived = |bg: Rgb, fg: Rgb, accent: Option<Rgb>, source: Source| {
        let (tokens, adjusted) = derive(bg, fg, accent);
        Resolved { tokens, source, osc, adjusted }
    };

    match mode {
        Mode::Light => designed(false, Source::Override),
        Mode::Dark => designed(true, Source::Override),
        Mode::Custom { fg, bg, accent } => derived(bg, fg, accent, Source::Override),
        Mode::Auto => match (term.fg, term.bg) {
            (Some(fg), Some(bg)) => derived(bg, fg, None, Source::Verified),
            (None, Some(bg)) => {
                // Only the background is known: borrow a legible foreground from the matching theme.
                let fg = palette(bg.luminance() < 0.4).text_primary;
                derived(bg, fg, None, Source::Approximate)
            }
            (Some(fg), None) => {
                let bg = palette(fg.luminance() >= 0.4).surface_base;
                derived(bg, fg, None, Source::Approximate)
            }
            (None, None) => match term.colorfgbg_bg {
                Some(i) => designed(i <= 6 || i == 8, Source::EnvHint),
                None => designed(true, Source::Fallback),
            },
        },
    }
}

impl Resolved {
    /// `key value` lines for the shell adapter.
    pub fn lines(&self) -> String {
        let mut out = format!("source {}\nosc {}\n", self.source.as_str(), self.osc.as_str());
        for (name, c) in self.tokens.list() {
            out.push_str(&format!("{name} {}\n", c.hex()));
        }
        if !self.adjusted.is_empty() {
            let names: Vec<_> = self.adjusted.iter().map(|a| a.token).collect();
            out.push_str(&format!("adjusted {}\n", names.join(" ")));
        }
        out
    }

    pub fn json(&self) -> Value {
        let tokens: serde_json::Map<String, Value> =
            self.tokens.list().iter().map(|(n, c)| (n.to_string(), json!(c.hex()))).collect();
        json!({
            "source": self.source.as_str(),
            "osc_color_query": self.osc.as_str(),
            "tokens": tokens,
            "adjusted": self.adjusted.iter().map(|a| json!({"token": a.token, "from": a.from.hex(), "to": a.to.hex()})).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(h: &str) -> Rgb {
        Rgb::from_hex(h).unwrap()
    }

    fn term(fg: Option<&str>, bg: Option<&str>) -> Terminal {
        Terminal { fg: fg.map(rgb), bg: bg.map(rgb), colorfgbg_bg: None, probed: true }
    }

    /// Every text-like token meets 4.5:1 on both surfaces.
    fn assert_readable(t: &Tokens) {
        for (name, c) in [("primary", t.text_primary), ("muted", t.text_muted), ("accent", t.accent), ("match", t.text_match)] {
            for (sname, s) in [("base", t.surface_base), ("selected", t.surface_selected)] {
                assert!(c.contrast(s) >= 4.5, "{name} on {sname}: {:.2}", c.contrast(s));
            }
        }
        assert!(t.focus_ring.contrast(t.surface_base) >= 3.0 && t.focus_ring.contrast(t.surface_selected) >= 3.0);
    }

    // Expected values were produced by running the prototype's own JS `derive()`.
    const GOLDEN: &[(&str, &str, &str, &str, &[&str])] = &[
        // (name, bg, fg, expected "base sel border primary muted accent", adjusted)
        ("green", "#0b130e", "#8ee6a8", "#0b130e #23392a #23392a #8ee6a8 #7ba888 #8ee6a8", &["text-muted"]),
        ("paper", "#f5f0e1", "#3b3a33", "#f5f0e1 #d4cfc2 #d4cfc2 #3b3a33 #5a5952 #3b3a33", &["text-muted"]),
        ("low", "#2a3230", "#7b8985", "#2a3230 #3c4542 #414947 #a8b2af #adb2b0 #a8b2af", &["text-primary", "text-muted"]),
        ("solar", "#002b36", "#93a1a1", "#002b36 #1a4049 #1c414a #99a7a7 #9aa9ad #99a7a7", &["text-primary", "text-muted"]),
    ];

    #[test]
    fn matches_prototype_derive() {
        for &(name, bg, fg, want, want_adj) in GOLDEN {
            let r = resolve(Mode::Auto, &term(Some(fg), Some(bg)));
            let t = &r.tokens;
            let got = [t.surface_base, t.surface_selected, t.border_subtle, t.text_primary, t.text_muted, t.accent]
                .map(Rgb::hex)
                .join(" ");
            assert_eq!(got, want, "{name}");
            let adj: Vec<_> = r.adjusted.iter().map(|a| a.token).collect();
            assert_eq!(adj, want_adj, "{name}");
            assert_readable(t);
        }
    }

    #[test]
    fn custom_accent_matches_prototype_derive() {
        let r = resolve(Mode::Custom { fg: rgb("#e6e6e6"), bg: rgb("#101418"), accent: Some(rgb("#7aa2f7")) }, &Terminal::default());
        let t = &r.tokens;
        let got = [t.surface_base, t.surface_selected, t.border_subtle, t.text_primary, t.text_muted, t.accent].map(Rgb::hex).join(" ");
        assert_eq!(got, "#101418 #232e40 #373a3d #e6e6e6 #949698 #7aa2f7");
    }

    #[test]
    fn low_contrast_palette_is_adjusted_and_reported() {
        let r = resolve(Mode::Auto, &term(Some("#7b8985"), Some("#2a3230")));
        assert_eq!(r.source, Source::Verified);
        assert!(!r.adjusted.is_empty());
        assert!(r.adjusted.iter().any(|a| a.token == "text-primary"));
        assert_ne!(r.tokens.text_primary, rgb("#7b8985"));
    }

    #[test]
    fn sources_follow_what_the_terminal_answered() {
        let both = resolve(Mode::Auto, &term(Some("#c9e8d2"), Some("#0d1210")));
        assert_eq!((both.source, both.osc), (Source::Verified, OscStatus::Verified));

        let bg_only = resolve(Mode::Auto, &term(None, Some("#f5f0e1")));
        assert_eq!((bg_only.source, bg_only.osc), (Source::Approximate, OscStatus::Unknown));
        assert_eq!(bg_only.tokens.surface_base, rgb("#f5f0e1"));
        assert!(bg_only.tokens.text_primary.luminance() < 0.4, "light background gets a dark foreground");
        assert_readable(&bg_only.tokens);

        let none = resolve(Mode::Auto, &term(None, None));
        assert_eq!((none.source, none.osc), (Source::Fallback, OscStatus::Unavailable));
        assert_eq!(none.tokens, palette(true));

        let unprobed = resolve(Mode::Auto, &Terminal::default());
        assert_eq!(unprobed.osc, OscStatus::Unknown);
    }

    #[test]
    fn env_hint_picks_light_or_dark() {
        let hint = |i| resolve(Mode::Auto, &Terminal { colorfgbg_bg: Some(i), ..Default::default() });
        assert_eq!((hint(0).source, hint(0).tokens), (Source::EnvHint, palette(true)));
        assert_eq!(hint(15).tokens, palette(false));
        assert_eq!(parse_colorfgbg("15;0"), Some(0));
        assert_eq!(parse_colorfgbg("0;default;15"), Some(15));
        assert_eq!(parse_colorfgbg("default"), None);
    }

    #[test]
    fn overrides_beat_the_terminal() {
        let t = term(Some("#c9e8d2"), Some("#0d1210"));
        let light = resolve(Mode::Light, &t);
        assert_eq!((light.source, light.tokens), (Source::Override, palette(false)));
        let custom = resolve(Mode::Custom { fg: rgb("#e6e6e6"), bg: rgb("#101418"), accent: Some(rgb("#7aa2f7")) }, &t);
        assert_eq!(custom.source, Source::Override);
        assert_eq!(custom.tokens.surface_base, rgb("#101418"));
        assert_readable(&custom.tokens);
        // Even the terminal's own accent is moved when it fails contrast.
        let weak = resolve(Mode::Custom { fg: rgb("#e6e6e6"), bg: rgb("#101418"), accent: Some(rgb("#1a2230")) }, &t);
        assert!(weak.adjusted.iter().any(|a| a.token == "accent"));
        assert_readable(&weak.tokens);
    }

    #[test]
    fn design_system_palettes_are_readable() {
        assert_readable(&palette(true));
        assert_readable(&palette(false));
    }

    #[test]
    fn config_modes() {
        let cfg = |s: &str| serde_json::from_str::<ThemeConfig>(s).unwrap().mode();
        assert_eq!(cfg(r#"{"mode":"dark"}"#), Mode::Dark);
        assert_eq!(cfg(r#"{"mode":"auto"}"#), Mode::Auto);
        assert_eq!(
            cfg(r##"{"mode":"custom","foreground":"#ffffff","background":"#000000"}"##),
            Mode::Custom { fg: rgb("#ffffff"), bg: rgb("#000000"), accent: None }
        );
        assert_eq!(cfg(r##"{"mode":"custom","foreground":"nope","background":"#000000"}"##), Mode::Auto);
        assert_eq!(cfg(r#"{"mode":"custom"}"#), Mode::Auto);
    }

    #[test]
    fn shell_lines_and_json_expose_the_report() {
        let r = resolve(Mode::Auto, &term(Some("#7b8985"), Some("#2a3230")));
        assert!(r.lines().starts_with("source verified\nosc verified\nsurface-base #2a3230\n"));
        assert!(r.lines().contains("\nadjusted text-primary"));
        assert_eq!(r.json()["adjusted"][0]["token"], "text-primary");
    }
}
