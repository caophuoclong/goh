//! Parse OSC 10 / OSC 11 colour replies read back from the terminal.
//!
//! Input is untrusted terminal bytes: it may hold typeahead, other sequences, truncated or
//! oversized payloads. Only well-formed replies are used; everything else is ignored.

use crate::color::Rgb;

/// Longest payload we accept (`rgb:ffff/ffff/ffff` is 18 bytes).
const MAX_PAYLOAD: usize = 64;
/// Longest reply buffer worth scanning.
pub const MAX_INPUT: usize = 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Replies {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
}

impl Replies {
    pub fn complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some()
    }
}

pub fn parse_replies(bytes: &[u8]) -> Replies {
    let bytes = &bytes[..bytes.len().min(MAX_INPUT)];
    let mut out = Replies::default();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != 0x1b || bytes[i + 1] != b']' {
            i += 1;
            continue;
        }
        i += 2;
        let ps_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() && i - ps_start < 3 {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b';' || i == ps_start {
            continue;
        }
        let ps = &bytes[ps_start..i];
        i += 1;
        // Payload runs to BEL or ESC \ ; anything longer or unterminated is dropped.
        let start = i;
        while i < bytes.len() && i - start <= MAX_PAYLOAD && bytes[i] != 0x07 && bytes[i] != 0x1b {
            i += 1;
        }
        let terminated = i < bytes.len()
            && i - start <= MAX_PAYLOAD
            && (bytes[i] == 0x07 || bytes.get(i + 1) == Some(&b'\\'));
        if !terminated {
            continue;
        }
        let colour = std::str::from_utf8(&bytes[start..i]).ok().and_then(Rgb::parse);
        match (ps, colour) {
            (b"10", Some(c)) if out.fg.is_none() => out.fg = Some(c),
            (b"11", Some(c)) if out.bg.is_none() => out.bg = Some(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FG: Rgb = Rgb([0xc9, 0xe8, 0xd2]);
    const BG: Rgb = Rgb([0x0d, 0x12, 0x10]);

    #[test]
    fn both_replies_with_bel_or_st_and_any_bit_depth() {
        let r = parse_replies(b"\x1b]10;rgb:c9/e8/d2\x07\x1b]11;rgb:0d0d/1212/1010\x1b\\");
        assert_eq!(r, Replies { fg: Some(FG), bg: Some(BG) });
        assert!(r.complete());
        let r = parse_replies(b"\x1b]11;#0d1210\x1b\\");
        assert_eq!(r, Replies { fg: None, bg: Some(BG) });
    }

    #[test]
    fn ignores_typeahead_noise_and_other_oscs() {
        let r = parse_replies(b"git ch\x1b]4;1;rgb:ff/00/00\x07\x1b[A\x1b]11;rgb:0d/12/10\x07zzz");
        assert_eq!(r, Replies { fg: None, bg: Some(BG) });
    }

    #[test]
    fn malformed_or_unterminated_replies_are_dropped() {
        for bad in [
            &b"\x1b]10;rgb:c9/e8/d2"[..],
            b"\x1b]10;rgb:c9/e8\x07",
            b"\x1b]10;rgb:zz/e8/d2\x07",
            b"\x1b]10;\x07",
            b"\x1b]10rgb:c9/e8/d2\x07",
            b"\x1b]1000;rgb:c9/e8/d2\x07",
            b"\x1b]10;\xff\xfe\x07",
        ] {
            assert_eq!(parse_replies(bad), Replies::default(), "{bad:?}");
        }
        let long = [b"\x1b]10;".as_slice(), &[b'a'; 200], b"\x07"].concat();
        assert_eq!(parse_replies(&long), Replies::default());
    }

    #[test]
    fn first_reply_wins_and_garbage_never_panics() {
        let r = parse_replies(b"\x1b]10;#ffffff\x07\x1b]10;#000000\x07");
        assert_eq!(r.fg, Some(Rgb([255, 255, 255])));
        for n in 0..=255u8 {
            let _ = parse_replies(&[0x1b, b']', b'1', b'0', b';', n, 0x1b, n]);
        }
        let _ = parse_replies(&[0x1b; 5000]);
    }
}
