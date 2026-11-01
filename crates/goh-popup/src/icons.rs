//! The popup's icons: the Nerd Font set of `goh_theme::icons`, drawn with a bundled subset of Nerd Fonts' symbols
//! font (`scripts/subset-nerd-font.sh`), so they show whatever font the popup uses. No remote or spec-provided assets.

use goh_theme::icons;

/// The bundled symbols font; where it comes from and its licences: `third_party/nerd-fonts`.
pub const FONT: &[u8] = include_bytes!("../assets/nerd-symbols.ttf");

/// One icon: its glyph, and the brand whose colour it takes (`goh_theme::icons::brand_color`) if it is a logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon {
    pub glyph: &'static str,
    pub brand: Option<&'static str>,
}

/// The icon of a suggestion; `None` for a frame without kinds (an adapter from before them).
pub fn row(kind: &str, label: &str) -> Option<Icon> {
    if kind.is_empty() {
        return None;
    }
    let brand = (kind == "command").then(|| goh_theme::brand(label)).flatten().filter(|b| icons::brand_glyph(b).is_some());
    Some(Icon { glyph: icons::glyph(kind, label), brand })
}

/// The logo of the command being completed, for the footer.
pub fn context(context: &str) -> Option<Icon> {
    if context.is_empty() {
        return None;
    }
    let brand = goh_theme::brand(context).filter(|b| icons::brand_glyph(b).is_some());
    Some(Icon { glyph: brand.and_then(icons::brand_glyph).unwrap_or(icons::TERMINAL), brand })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_their_kind_and_the_footer_its_command() {
        assert_eq!(row("command", "kubectl"), Some(Icon { glyph: icons::brand_glyph("kubernetes").unwrap(), brand: Some("kubernetes") }));
        assert_eq!(row("command", "pnpm"), Some(Icon { glyph: icons::TERMINAL, brand: None }), "no Nerd Font logo");
        assert_eq!(row("flag", "--force"), Some(Icon { glyph: icons::glyph("flag", ""), brand: None }));
        assert_eq!(row("", "x"), None);
        assert_eq!(context("python3"), Some(Icon { glyph: icons::brand_glyph("python").unwrap(), brand: Some("python") }));
        assert_eq!(context("custom"), Some(Icon { glyph: icons::TERMINAL, brand: None }));
        assert_eq!(context(""), None);
    }

    #[test]
    fn the_bundled_font_has_every_icon_in_one_cell() {
        let face = ttf_parser::Face::parse(FONT, 0).unwrap();
        let cell = face.glyph_index(icons::TERMINAL.chars().next().unwrap()).and_then(|id| face.glyph_hor_advance(id));
        for g in icons::all() {
            let c = g.chars().next().unwrap();
            let id = face.glyph_index(c).unwrap_or_else(|| panic!("U+{:X} is not in the font: run scripts/subset-nerd-font.sh", c as u32));
            assert_eq!(face.glyph_hor_advance(id), cell, "U+{:X} is one cell wide", c as u32);
        }
    }
}
