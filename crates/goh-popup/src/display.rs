//! Which display mode a session may use (ADR-003). The overlay is capability-based: it is drawn
//! only when every precondition is *verified*; otherwise the inline fallback is used, with a
//! reason for diagnostics. A forced `overlay` setting cannot override a failed precondition,
//! because an overlay that is not at the caret is worse than none.

use serde::Deserialize;

use crate::model::WIDTH_MIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Auto,
    Overlay,
    Inline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// A trusted source gave the caret position for this session (see docs/18, S-04/05/06).
    Verified,
    Unverified,
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub anchor: Anchor,
    /// The window can be shown without taking keyboard focus from the terminal.
    pub focus_safe: bool,
    pub available_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Overlay,
    Inline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub display: Display,
    pub reason: &'static str,
}

pub fn resolve(mode: Mode, caps: &Capabilities) -> Decision {
    let inline = |reason| Decision { display: Display::Inline, reason };
    if mode == Mode::Inline {
        return inline("inline selected in settings");
    }
    if caps.available_width < WIDTH_MIN {
        return inline("less than 320 px available");
    }
    if caps.anchor != Anchor::Verified {
        return inline(if mode == Mode::Overlay {
            "overlay requested but the caret position is not verified"
        } else {
            "caret position not verified"
        });
    }
    if !caps.focus_safe {
        return inline("overlay cannot be shown without taking focus");
    }
    Decision { display: Display::Overlay, reason: "anchor verified and focus preserved" }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: Capabilities = Capabilities { anchor: Anchor::Verified, focus_safe: true, available_width: 600.0 };

    #[test]
    fn overlay_only_when_everything_is_verified() {
        for mode in [Mode::Auto, Mode::Overlay] {
            assert_eq!(resolve(mode, &OK).display, Display::Overlay);
        }
        assert_eq!(resolve(Mode::Inline, &OK).display, Display::Inline);
    }

    #[test]
    fn each_failed_precondition_falls_back_with_a_reason() {
        let cases = [
            (Capabilities { anchor: Anchor::Unverified, ..OK }, "caret position not verified"),
            (Capabilities { focus_safe: false, ..OK }, "overlay cannot be shown without taking focus"),
            (Capabilities { available_width: 319.0, ..OK }, "less than 320 px available"),
        ];
        for (caps, reason) in cases {
            let d = resolve(Mode::Auto, &caps);
            assert_eq!((d.display, d.reason), (Display::Inline, reason));
        }
    }

    #[test]
    fn forcing_overlay_does_not_bypass_an_unverified_anchor() {
        let d = resolve(Mode::Overlay, &Capabilities { anchor: Anchor::Unverified, ..OK });
        assert_eq!(d.display, Display::Inline);
        assert!(d.reason.starts_with("overlay requested"));
    }

    #[test]
    fn mode_parses_from_config_strings() {
        assert_eq!(serde_json::from_str::<Mode>("\"overlay\"").unwrap(), Mode::Overlay);
        assert_eq!(Mode::default(), Mode::Auto);
    }
}
