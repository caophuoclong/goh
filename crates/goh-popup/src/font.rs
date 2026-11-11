//! Which font the terminal uses, so the popup can draw in it. Pure parsing here; the reading of files
//! and the font lookup live in the binary. Only terminals whose font can be read are covered.

use serde_json::Value;

/// Font family names to try in order (CSS-style fallbacks, or one PostScript name) and the size in points.
#[derive(Debug, Clone, PartialEq)]
pub struct TermFont {
    pub families: Vec<String>,
    pub size: Option<f32>,
}

const GENERIC: [&str; 5] = ["monospace", "ui-monospace", "sans-serif", "serif", "system-ui"];

/// `JetBrains Mono` or `JetBrains Mono:13` (the `GOH_POPUP_FONT` override).
pub fn parse_override(spec: &str) -> Option<TermFont> {
    let (name, size) = match spec.rsplit_once(':') {
        Some((n, s)) if s.trim().parse::<f32>().is_ok() => (n, s.trim().parse().ok()),
        _ => (spec, None),
    };
    let name = name.trim();
    (!name.is_empty()).then(|| TermFont { families: vec![name.to_string()], size })
}

/// A CSS `font-family` list: `Menlo, 'Fira Code', monospace` -> `["Menlo", "Fira Code"]`.
pub fn css_families(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut cur, mut quote) = (String::new(), None::<char>);
    let mut push = |cur: &mut String| {
        let name = cur.trim().trim_matches(['\'', '"']).trim().to_string();
        if !name.is_empty() && !GENERIC.contains(&name.to_lowercase().as_str()) {
            out.push(name);
        }
        cur.clear();
    };
    for c in list.chars() {
        match (c, quote) {
            ('\'' | '"', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            (',', None) => push(&mut cur),
            _ => cur.push(c),
        }
    }
    push(&mut cur);
    out
}

/// VS Code settings are JSON with comments and trailing commas.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut i, mut in_str) = (String::new(), 0, false);
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' && chars[i + 1..].iter().find(|c| !c.is_whitespace()).is_some_and(|n| matches!(n, '}' | ']')) {
            // trailing comma
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// The terminal font from VS Code's `settings.json`: `terminal.integrated.fontFamily`, else
/// `editor.fontFamily`; size `terminal.integrated.fontSize`, else 14 (VS Code's default).
pub fn vscode(settings: &str) -> Option<TermFont> {
    let v: Value = serde_json::from_str(&strip_jsonc(settings)).ok()?;
    let families = ["terminal.integrated.fontFamily", "editor.fontFamily"].iter().find_map(|k| v.get(k)?.as_str().map(css_families).filter(|f| !f.is_empty()))?;
    let size = v.get("terminal.integrated.fontSize").and_then(Value::as_f64).unwrap_or(14.0) as f32;
    Some(TermFont { families, size: Some(size) })
}

/// iTerm2's profile font from its profiles (`New Bookmarks` as JSON) and the default profile's `Guid`:
/// `Normal Font` is `PostScriptName Size`.
pub fn iterm(profiles: &str, default_guid: Option<&str>, profile_name: Option<&str>) -> Option<TermFont> {
    let v: Value = serde_json::from_str(profiles).ok()?;
    let profiles = v.as_array()?;
    let by_name = profile_name.and_then(|n| profiles.iter().find(|p| p.get("Name").and_then(Value::as_str) == Some(n)));
    let profile = by_name.or_else(|| profiles.iter().find(|p| p.get("Guid").and_then(Value::as_str) == default_guid)).or(profiles.first())?;
    let (name, size) = profile.get("Normal Font")?.as_str()?.rsplit_once(' ')?;
    Some(TermFont { families: vec![name.to_string()], size: size.parse().ok() })
}

/// Windows Terminal's font when its `settings.json` (JSON with comments) is missing or says nothing: Cascadia Mono 12.
pub const WINDOWS_TERMINAL_DEFAULT: (&str, f32) = ("Cascadia Mono", 12.0);

/// The font of a Windows Terminal profile: the one whose `guid` is `profile_id` (`WT_PROFILE_ID`), else the default
/// profile; its `font.face`/`font.size` (or the older `fontFace`/`fontSize`), else `profiles.defaults`, else
/// Windows Terminal's default. Sizes there are typographic points, converted to the popup's 96-per-inch points.
pub fn windows_terminal(settings: Option<&str>, profile_id: Option<&str>) -> TermFont {
    let v: Value = settings.and_then(|s| serde_json::from_str(&strip_jsonc(s)).ok()).unwrap_or(Value::Null);
    let profiles = v.get("profiles");
    // `profiles` is an object with `defaults` and `list`, or (in old files) the list itself.
    let list = profiles.and_then(|p| p.get("list").or(Some(p))).and_then(Value::as_array);
    let by_guid = |id: &str| list?.iter().find(|p| p.get("guid").and_then(Value::as_str).is_some_and(|g| g.eq_ignore_ascii_case(id)));
    let profile = profile_id.and_then(by_guid).or_else(|| v.get("defaultProfile").and_then(Value::as_str).and_then(by_guid));
    let defaults = profiles.and_then(|p| p.get("defaults"));
    let pick = |font_key: &str, old_key: &str| {
        [profile, defaults].into_iter().flatten().find_map(|p| p.get("font").and_then(|f| f.get(font_key)).or_else(|| p.get(old_key)).cloned())
    };
    let families = pick("face", "fontFace").and_then(|f| f.as_str().map(css_families)).filter(|f| !f.is_empty());
    let size = pick("size", "fontSize").and_then(|s| s.as_f64()).map(|s| s as f32).unwrap_or(WINDOWS_TERMINAL_DEFAULT.1);
    TermFont {
        families: families.unwrap_or_else(|| vec![WINDOWS_TERMINAL_DEFAULT.0.to_string()]),
        size: Some(size * 96.0 / 72.0),
    }
}

/// The file of `family` among the fonts Windows lists in its registry (value name, file): names such as
/// `Cascadia Mono Regular (TrueType)` or `JetBrainsMono Nerd Font Mono Regular (TrueType)`. The regular face wins.
pub fn windows_font_file<'a>(entries: &'a [(String, String)], family: &str) -> Option<&'a str> {
    let want = normalize(family);
    let face = |name: &str| normalize(name.rsplit_once(" (").map_or(name, |(n, _)| n));
    let regular = format!("{want}regular");
    entries.iter().find(|(n, _)| face(n) == regular).or_else(|| entries.iter().find(|(n, _)| face(n) == want)).map(|(_, f)| f.as_str())
}

/// Lowercase letters and digits only, so "JetBrains Mono" matches "JetBrainsMono-Regular".
pub fn normalize(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Advance of `M` in points at `size`, from a font file; `None` if it cannot be parsed.
pub fn cell_width(font_bytes: &[u8], size: f32) -> Option<f32> {
    let face = ttf_parser::Face::parse(font_bytes, 0).ok()?;
    let advance = face.glyph_hor_advance(face.glyph_index('M')?)?;
    Some(f32::from(advance) / f32::from(face.units_per_em()) * size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_has_an_optional_size() {
        assert_eq!(parse_override("JetBrains Mono:13").unwrap(), TermFont { families: vec!["JetBrains Mono".into()], size: Some(13.0) });
        assert_eq!(parse_override("Menlo").unwrap().size, None);
        assert_eq!(parse_override("  "), None);
    }

    #[test]
    fn css_lists_keep_names_and_drop_generics() {
        assert_eq!(css_families("Menlo, 'Fira Code', \"Source, Code\", monospace"), ["Menlo", "Fira Code", "Source, Code"]);
        assert!(css_families("monospace").is_empty());
    }

    #[test]
    fn vscode_settings_with_comments_and_trailing_commas() {
        let text = r#"{
            // my font
            "editor.fontFamily": "Fira Code, monospace", /* fallback */
            "terminal.integrated.fontFamily": "'JetBrainsMono Nerd Font', Menlo",
            "terminal.integrated.fontSize": 13,
            "url": "http://x//y",
        }"#;
        assert_eq!(vscode(text).unwrap(), TermFont { families: vec!["JetBrainsMono Nerd Font".into(), "Menlo".into()], size: Some(13.0) });
        assert_eq!(vscode(r#"{"editor.fontFamily":"Fira Code"}"#).unwrap().size, Some(14.0), "VS Code's default size");
        assert_eq!(vscode("{}"), None);
    }

    #[test]
    fn iterm_uses_the_named_profile_else_the_default() {
        let profiles = r#"[
            {"Name":"Work","Guid":"A","Normal Font":"Menlo-Regular 12"},
            {"Name":"Default","Guid":"B","Normal Font":"JetBrainsMonoNF-Regular 14"}]"#;
        assert_eq!(iterm(profiles, Some("B"), Some("Work")).unwrap(), TermFont { families: vec!["Menlo-Regular".into()], size: Some(12.0) });
        assert_eq!(iterm(profiles, Some("B"), None).unwrap().families, ["JetBrainsMonoNF-Regular"]);
        assert_eq!(iterm(profiles, Some("B"), Some("Missing")).unwrap().size, Some(14.0));
        assert_eq!(iterm(profiles, None, None).unwrap().size, Some(12.0), "no default: the first profile");
    }

    #[test]
    fn windows_terminal_profile_font_else_defaults_else_cascadia() {
        let text = r#"{
            // Windows Terminal writes comments into new files
            "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
            "profiles": {
                "defaults": { "font": { "face": "Cascadia Code", "size": 11 } },
                "list": [
                    { "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "name": "Windows PowerShell" },
                    { "guid": "{2c4de342-38b7-51cf-b940-2309a097f518}", "name": "Ubuntu", "font": { "face": "JetBrainsMono Nerd Font, Consolas", "size": 13.5 } },
                    { "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}", "name": "Old", "fontFace": "Consolas", "fontSize": 10 },
                ]
            }
        }"#;
        let ubuntu = windows_terminal(Some(text), Some("{2C4DE342-38B7-51CF-B940-2309A097F518}"));
        assert_eq!(ubuntu, TermFont { families: vec!["JetBrainsMono Nerd Font".into(), "Consolas".into()], size: Some(18.0) });
        let shell = windows_terminal(Some(text), Some("{61c54bbd-c2c6-5271-96e7-009a87ff44bf}"));
        assert_eq!(shell.families, ["Cascadia Code"], "a profile without a font takes the defaults");
        assert_eq!(windows_terminal(Some(text), Some("{574e775e-4f2a-5b96-ac1e-a2962a402336}")).families, ["Consolas"]);
        assert_eq!(windows_terminal(Some(text), None).families, ["Cascadia Code"], "no profile id: the default profile");
        assert_eq!(windows_terminal(None, None), TermFont { families: vec!["Cascadia Mono".into()], size: Some(16.0) });
        assert_eq!(windows_terminal(Some("not json"), Some("{x}")).families, ["Cascadia Mono"]);
    }

    #[test]
    fn windows_font_files_come_from_registry_names() {
        let entries = [
            ("Cascadia Mono Bold (TrueType)".to_string(), r"C:\Windows\Fonts\CascadiaMono-Bold.ttf".to_string()),
            ("Cascadia Mono Regular (TrueType)".to_string(), r"C:\Windows\Fonts\CascadiaMono.ttf".to_string()),
            ("Consolas (TrueType)".to_string(), r"C:\Windows\Fonts\consola.ttf".to_string()),
        ];
        assert_eq!(windows_font_file(&entries, "Cascadia Mono"), Some(r"C:\Windows\Fonts\CascadiaMono.ttf"));
        assert_eq!(windows_font_file(&entries, "consolas"), Some(r"C:\Windows\Fonts\consola.ttf"));
        assert_eq!(windows_font_file(&entries, "Fira Code"), None);
    }

    #[test]
    fn names_compare_without_spaces_or_case() {
        assert_eq!(normalize("JetBrains Mono"), normalize("jetbrainsmono"));
    }
}
