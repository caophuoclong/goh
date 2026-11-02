//! Nerd Font icons, the one set every menu draws (docs/design-system/README.md, Iconography): the popup with its
//! bundled symbols font, the inline menu under `display.icons: "nerd"` with the terminal's. The zsh adapter keeps a
//! copy of these tables (`_goh_glyph`); a test below holds the two equal.

use crate::brand;

/// `nf-fa-terminal`: a command without a brand logo.
pub const TERMINAL: &str = "\u{f120}";

/// The logo of a brand (`brand()`), if Nerd Fonts has one.
pub fn brand_glyph(brand: &str) -> Option<&'static str> {
    Some(match brand {
        "docker" => "\u{e7b0}",
        "nodejs" => "\u{e718}",
        "go" => "\u{e627}",
        "git" => "\u{e702}",
        "npm" => "\u{e71e}",
        "python" => "\u{e73c}",
        "rust" => "\u{e7a8}",
        "kubernetes" => "\u{f10fe}",
        "yarn" => "\u{e6a7}",
        "terraform" => "\u{e69a}",
        "github" => "\u{f09b}",
        _ => return None,
    })
}

/// The colour a brand's logo is drawn in, as `#rrggbb`; `None` draws it in the text colour.
pub fn brand_color(brand: &str) -> Option<&'static str> {
    Some(match brand {
        "docker" => "#2496ed",
        "nodejs" => "#5fa04e",
        "go" => "#00add8",
        "git" => "#f05033",
        "npm" => "#cb3837",
        "python" => "#4b8bbe",
        "rust" => "#ce422b",
        "kubernetes" => "#326ce5",
        "yarn" => "#2c8ebb",
        "terraform" => "#7b42bc",
        _ => return None,
    })
}

/// The icon of a file, by its extension (case does not matter).
pub fn file_glyph(name: &str) -> &'static str {
    let base = name.rsplit('/').next().unwrap_or(name);
    let ext = base.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "rs" => "\u{e7a8}",
        "go" => "\u{e627}",
        "py" => "\u{e73c}",
        "js" | "mjs" | "cjs" => "\u{e60c}",
        "ts" | "mts" | "cts" => "\u{e628}",
        "jsx" | "tsx" => "\u{e7ba}",
        "json" => "\u{e60b}",
        "md" => "\u{e73e}",
        "toml" => "\u{e615}",
        "yml" | "yaml" => "\u{e6a8}",
        "sh" | "zsh" => "\u{e691}",
        "lock" => "\u{e672}",
        "html" => "\u{e60e}",
        "css" => "\u{e614}",
        "c" => "\u{e649}",
        "cpp" => "\u{e646}",
        "java" => "\u{e66d}",
        "rb" => "\u{e605}",
        "lua" => "\u{e620}",
        "txt" => "\u{e64e}",
        "php" => "\u{e73d}",
        "dart" => "\u{e798}",
        "swift" => "\u{f179}",
        "kt" | "kotlin" => "\u{e634}",
        "scala" => "\u{e737}",
        "clj" | "cljs" | "cljc" => "\u{e768}",
        "hs" | "lhs" => "\u{e777}",
        "erl" | "hrl" => "\u{e7b1}",
        "ex" | "exs" => "\u{e62d}",
        "r" => "\u{f25d}",
        "pl" | "pm" => "\u{e769}",
        "vim" => "\u{e62b}",
        "vue" => "\u{e6a0}",
        "sass" | "scss" => "\u{e603}",
        "less" => "\u{e758}",
        "xml" | "xsl" | "xslt" => "\u{e619}",
        "sql" | "sqlite" | "sqlite3" | "db" => "\u{f1c0}",
        "diff" | "patch" => "\u{f440}",
        "pdf" => "\u{f1c1}",
        "doc" | "docx" => "\u{f1c2}",
        "xls" | "xlsx" => "\u{f1c3}",
        "ppt" | "pptx" => "\u{f1c4}",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" => "\u{f1c5}",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "\u{f001}",
        "mp4" | "mov" | "avi" | "mkv" | "webm" => "\u{f03d}",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "\u{f410}",
        "ttf" | "otf" | "woff" | "woff2" => "\u{f031}",
        "log" => "\u{f18d}",
        "conf" | "ini" | "cfg" => "\u{e615}",
        "fish" | "bash" => "\u{e691}",
        "h" => "\u{e649}",
        "hpp" | "cc" | "cxx" => "\u{e646}",
        "markdown" => "\u{e73e}",
        "htm" => "\u{e60e}",
        "csv" | "tsv" => "\u{e64e}",
        _ => "\u{f016}",
    }
}

/// The icon before a suggestion's label. `kind` is the engine's, with `flag` for an option that takes no value.
pub fn glyph(kind: &str, label: &str) -> &'static str {
    match kind {
        "command" => brand(label).and_then(brand_glyph).unwrap_or(TERMINAL),
        "subcommand" => "\u{f060d}",
        "option" => "\u{f1de}",
        "flag" => "\u{f024}",
        "argument" => "\u{ea88}",
        "directory" => "\u{f07b}",
        "file" => file_glyph(label),
        "history" => "\u{f1da}",
        _ => "·",
    }
}

/// Every glyph above, for the font subset (`scripts/subset-nerd-font.sh`) and its test.
pub fn all() -> Vec<&'static str> {
    let brands = ["docker", "nodejs", "go", "git", "npm", "python", "rust", "kubernetes", "yarn", "terraform", "github"];
    let files = ["a.rs", "a.go", "a.py", "a.js", "a.ts", "a.json", "a.md", "a.toml", "a.yml", "a.sh", "a.lock", "a.html", "a.css", "a.c", "a.cpp", "a.java", "a.rb", "a.lua", "a.txt", "a"];
    let extra_files = ["a.jsx", "a.php", "a.dart", "a.swift", "a.kt", "a.scala", "a.clj", "a.hs", "a.erl", "a.ex", "a.r", "a.pl", "a.vim", "a.vue", "a.sass", "a.less", "a.xml", "a.db", "a.diff", "a.pdf", "a.doc", "a.xls", "a.ppt", "a.png", "a.mp3", "a.mp4", "a.zip", "a.ttf", "a.log", "a.conf", "a.fish", "a.h", "a.hpp", "a.markdown", "a.htm", "a.csv"];
    let kinds = ["subcommand", "option", "flag", "argument", "directory", "history"];
    let mut all: Vec<_> = brands.iter().filter_map(|b| brand_glyph(b)).collect();
    all.extend(files.iter().map(|f| file_glyph(f)));
    all.extend(extra_files.iter().map(|f| file_glyph(f)));
    all.extend(kinds.iter().map(|k| glyph(k, "")));
    all.push(TERMINAL);
    all.sort_unstable();
    all.dedup();
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn commands_show_their_brand_and_files_their_type() {
        assert_eq!(glyph("command", "kubectl"), brand_glyph("kubernetes").unwrap());
        assert_eq!(glyph("command", "pnpm"), TERMINAL, "pnpm has no Nerd Font logo");
        assert_eq!(glyph("command", "grep"), TERMINAL);
        assert_eq!(glyph("file", "src/Main.RS"), glyph("file", "lib.rs"));
        for ext in ["mjs", "cjs"] {
            assert_eq!(file_glyph(&format!("src/component.{ext}")), file_glyph("a.js"));
        }
        for ext in ["mts", "cts"] {
            assert_eq!(file_glyph(&format!("src/component.{ext}")), file_glyph("a.ts"));
        }
        for ext in ["jsx", "tsx", "JSX", "TSX"] {
            assert_eq!(file_glyph(&format!("src/component.{ext}")), "\u{e7ba}");
        }
        for (name, expected) in [("photo.PNG", "\u{f1c5}"), ("report.pdf", "\u{f1c1}"), ("archive.tar", "\u{f410}"), ("src/main.php", "\u{e73d}"), ("schema.sql", "\u{f1c0}")] {
            assert_eq!(file_glyph(name), expected, "{name}");
        }
        assert_eq!(glyph("file", "Makefile"), "\u{f016}");
        assert_eq!(glyph("mixed", "x"), "·");
    }

    /// `name $'…'` pairs of a zsh associative array in the adapter, with `$'\U…'` escapes decoded.
    fn zsh_table(src: &str, name: &str) -> BTreeMap<String, String> {
        let start = src.find(&format!("typeset -gA {name}=(")).expect(name);
        let body = &src[start..start + src[start..].find("\n)").unwrap()];
        body.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .filter_map(|w| Some((w[0].to_string(), zsh_string(w[1])?)))
            .collect()
    }

    fn zsh_string(word: &str) -> Option<String> {
        let inner = word.strip_prefix('$').unwrap_or(word).strip_prefix('\'')?.strip_suffix('\'')?;
        Some(match inner.strip_prefix("\\U") {
            Some(hex) => char::from_u32(u32::from_str_radix(hex, 16).ok()?)?.to_string(),
            None => inner.to_string(),
        })
    }

    #[test]
    fn the_zsh_adapter_draws_the_same_icons() {
        let src = include_str!("../../../adapters/zsh/goh.zsh");
        for (name, n) in [("_goh_nerd_brand", 11), ("_goh_brand_color", 10), ("_goh_nerd_ext", 109)] {
            assert_eq!(zsh_table(src, name).len(), n, "{name} read whole");
        }
        for (brand, g) in zsh_table(src, "_goh_nerd_brand") {
            assert_eq!(Some(g.as_str()), brand_glyph(&brand), "brand {brand}");
        }
        for (brand, c) in zsh_table(src, "_goh_brand_color") {
            assert_eq!(Some(c.as_str()), brand_color(&brand), "colour {brand}");
        }
        for (ext, g) in zsh_table(src, "_goh_nerd_ext") {
            assert_eq!(g, file_glyph(&format!("a.{ext}")), "file .{ext}");
        }
        let body = &src[src.find("_goh_glyph() {").unwrap()..];
        let body = &body[..body.find("\n}").unwrap()];
        for kind in ["subcommand", "option", "flag", "argument", "directory", "history"] {
            let line = body.lines().find(|l| l.trim_start().starts_with(&format!("{kind})"))).unwrap();
            let word = line.split_whitespace().find_map(|w| w.strip_prefix("_goh_g=")).unwrap();
            assert_eq!(zsh_string(word).as_deref(), Some(glyph(kind, "")), "kind {kind}");
        }
        assert!(body.contains(&format!(":-$'{TERMINAL}'}}")), "a command without a logo");
        assert!(body.contains(&format!(":-$'{}'}}", file_glyph("a"))), "a file of another type");
    }
}
