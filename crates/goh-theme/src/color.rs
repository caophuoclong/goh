//! Colour parsing and the contrast maths the resolver needs (WCAG relative luminance).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    /// `#RGB` or `#RRGGBB`.
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let h = s.strip_prefix('#')?;
        if !h.is_ascii() {
            return None;
        }
        match h.len() {
            3 => {
                let d = |i: usize| u8::from_str_radix(&h[i..=i], 16).ok().map(|v| v * 17);
                Some(Rgb([d(0)?, d(1)?, d(2)?]))
            }
            6 => {
                let d = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
                Some(Rgb([d(0)?, d(2)?, d(4)?]))
            }
            _ => None,
        }
    }

    /// X11 `rgb:R/G/B` where each part has 1–4 hex digits (4-, 8-, 12- or 16-bit colour).
    pub fn from_x11(s: &str) -> Option<Rgb> {
        let parts: Vec<&str> = s.strip_prefix("rgb:")?.split('/').collect();
        if parts.len() != 3 {
            return None;
        }
        let mut out = [0u8; 3];
        for (o, p) in out.iter_mut().zip(parts) {
            if p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let v = u32::from_str_radix(p, 16).ok()?;
            let max = (1u32 << (4 * p.len())) - 1;
            *o = ((v as f64) / (max as f64) * 255.0).round() as u8;
        }
        Some(Rgb(out))
    }

    pub fn parse(s: &str) -> Option<Rgb> {
        Rgb::from_x11(s).or_else(|| Rgb::from_hex(s))
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }

    pub fn luminance(self) -> f64 {
        let c = self.0.map(|v| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        });
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `self` moved `t` of the way towards `b`, rounded per channel.
    pub fn mix(self, b: Rgb, t: f64) -> Rgb {
        let mut out = [0u8; 3];
        for (i, o) in out.iter_mut().enumerate() {
            let v = self.0[i] as f64;
            *o = (v + (b.0[i] as f64 - v) * t).round().clamp(0.0, 255.0) as u8;
        }
        Rgb(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_x11_in_all_widths() {
        assert_eq!(Rgb::from_hex("#1d3027"), Some(Rgb([0x1d, 0x30, 0x27])));
        assert_eq!(Rgb::from_hex("#fa0"), Some(Rgb([0xff, 0xaa, 0x00])));
        assert_eq!(Rgb::from_x11("rgb:f/0/8"), Some(Rgb([255, 0, 136])));
        assert_eq!(Rgb::from_x11("rgb:1d/30/27"), Some(Rgb([0x1d, 0x30, 0x27])));
        assert_eq!(Rgb::from_x11("rgb:1d1d/3030/2727"), Some(Rgb([0x1d, 0x30, 0x27])));
        assert_eq!(Rgb::from_x11("rgb:ffff/0000/8000"), Some(Rgb([255, 0, 128])));
    }

    #[test]
    fn rejects_malformed_colours() {
        for bad in ["", "#", "#12", "#12345", "#gggggg", "#é12", "rgb:", "rgb:1/2", "rgb:1/2/3/4", "rgb:12345/0/0", "rgb:x/0/0", "rgb:/0/0", "red"] {
            assert_eq!(Rgb::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn contrast_extremes() {
        let (b, w) = (Rgb([0, 0, 0]), Rgb([255, 255, 255]));
        assert!((b.contrast(w) - 21.0).abs() < 1e-9);
        assert!((w.contrast(w) - 1.0).abs() < 1e-9);
    }
}
