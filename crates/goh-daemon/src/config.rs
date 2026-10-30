//! The settings file (`goh config`): where it is, a first-run template, what the adapters read from it, and
//! opening it in an editor. Environment variables still win over the file, so one-off overrides keep working.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

/// Written the first time `goh config` runs: the schema-validated example (docs/examples/config.json) with a comment
/// on every setting.
pub const TEMPLATE: &str = include_str!("../../../docs/examples/config.jsonc");

pub fn path() -> io::Result<PathBuf> {
    goh_theme::config_path().ok_or_else(|| io::Error::other("no HOME and no GOH_CONFIG: cannot place the config file"))
}

/// Create the file with defaults if it does not exist yet.
pub fn ensure() -> io::Result<PathBuf> {
    let path = path()?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, TEMPLATE)?;
    }
    Ok(path)
}

/// A file written before the template had comments gets them: rewritten from the template with its own settings,
/// after a copy is kept next to it. `Ok(Some(copy))` when that happened. A file that already has comments, or one
/// that cannot be read or checked, is left alone.
pub fn add_comments(path: &Path) -> io::Result<Option<PathBuf>> {
    let text = std::fs::read_to_string(path)?;
    let Some(commented) = commented(&text) else { return Ok(None) };
    let name = path.file_name().map_or_else(|| "config.json".into(), |n| n.to_string_lossy().into_owned());
    let copy = (1..).map(|n| path.with_file_name(if n == 1 { format!("{name}.bak") } else { format!("{name}.bak.{n}") })).find(|p| !p.exists()).expect("a free name");
    std::fs::copy(path, &copy)?;
    std::fs::write(path, commented)?;
    Ok(Some(copy))
}

/// `text` (a file without comments) as the commented template filled with its settings; `None` unless that keeps
/// every setting it has.
fn commented(text: &str) -> Option<String> {
    if goh_theme::strip_comments(text) != text || validate(text).is_err() {
        return None;
    }
    let old = goh_theme::parse_config(text).ok()?;
    old.as_object()?;
    let new = with_comments(&old);
    let back = goh_theme::parse_config(&new).ok()?;
    keeps(&back, &old).then_some(new)
}

/// Every setting in `old` is in `new`, with the same value.
fn keeps(new: &Value, old: &Value) -> bool {
    match (new, old) {
        (Value::Object(new), Value::Object(old)) => old.iter().all(|(k, v)| new.get(k).is_some_and(|n| keeps(n, v))),
        _ => new == old,
    }
}

/// A setting of the template: the comment lines above it, its key, and its default (as JSON text) or what is in it.
struct Item {
    comments: Vec<String>,
    key: String,
    body: Body,
}

enum Body {
    Default(String),
    Section(Vec<Item>),
}

/// The template read back as its settings, after the comment lines that come before `{`. It has one setting per
/// line, which the tests hold it to.
fn template() -> (Vec<&'static str>, Vec<Item>) {
    let mut lines = TEMPLATE.lines();
    let header = lines.by_ref().take_while(|l| l.trim() != "{").collect();
    (header, items(&mut lines))
}

fn items<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Vec<Item> {
    let (mut out, mut comments) = (Vec::new(), Vec::new());
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.starts_with("//") {
            comments.push(line.to_string());
        } else if line.starts_with('}') {
            break;
        } else if let Some((key, rest)) = line.split_once(':') {
            let rest = rest.trim().trim_end_matches(',');
            let body = if rest == "{" { Body::Section(items(lines)) } else { Body::Default(rest.to_string()) };
            out.push(Item { comments: std::mem::take(&mut comments), key: key.trim().trim_matches('"').to_string(), body });
        }
    }
    out
}

/// The commented template with the settings in `v`: its values where it has them, the defaults elsewhere, and
/// settings the template does not name (a custom theme's colours, ...) kept after the ones it does.
fn with_comments(v: &Value) -> String {
    let (header, items) = template();
    let mut out = header.iter().map(|l| format!("{l}\n")).collect::<String>();
    out.push_str("{\n");
    render(&items, v.as_object(), 1, &mut out);
    out.push_str("}\n");
    out
}

fn render(items: &[Item], v: Option<&Map<String, Value>>, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let others: Vec<(&String, &Value)> = v.into_iter().flatten().filter(|(k, _)| !items.iter().any(|i| &i.key == *k)).collect();
    let total = items.len() + others.len();
    let end = |i: usize| if i + 1 < total { ",\n" } else { "\n" };
    for (i, item) in items.iter().enumerate() {
        if depth == 1 && i > 0 {
            out.push('\n');
        }
        for c in &item.comments {
            out.push_str(&format!("{pad}{c}\n"));
        }
        out.push_str(&format!("{pad}{}: ", Value::from(item.key.as_str())));
        let mine = v.and_then(|m| m.get(&item.key));
        match (&item.body, mine) {
            (Body::Section(inner), _) => {
                out.push_str("{\n");
                render(inner, mine.and_then(Value::as_object), depth + 1, out);
                out.push_str(&format!("{pad}}}"));
            }
            (Body::Default(_), Some(mine)) => out.push_str(&mine.to_string()),
            (Body::Default(default), None) => out.push_str(default),
        }
        out.push_str(end(i));
    }
    for (i, (k, value)) in others.iter().enumerate() {
        out.push_str(&format!("{pad}{}: {value}{}", Value::from(k.as_str()), end(items.len() + i)));
    }
}

/// `Err` carries a message fit to show: the syntax error, or the value that cannot be used.
pub fn validate(text: &str) -> Result<(), String> {
    let v: Value = goh_theme::parse_config(text).map_err(|e| format!("not valid JSON ({e})"))?;
    if let Some(mode) = v.pointer("/display/mode") {
        if !matches!(mode.as_str(), Some("auto" | "overlay" | "inline")) {
            return Err(format!("display.mode must be \"auto\", \"overlay\" or \"inline\", not {mode}"));
        }
    }
    if let Some(n) = v.pointer("/display/maxSuggestions") {
        if !n.as_u64().is_some_and(|n| (1..=50).contains(&n)) {
            return Err(format!("display.maxSuggestions must be a whole number from 1 to 50, not {n}"));
        }
    }
    if let Some(live) = v.pointer("/display/live") {
        if !live.is_boolean() {
            return Err(format!("display.live must be true or false, not {live}"));
        }
    }
    for (path, name) in [("/display/predict", "display.predict"), ("/privacy/recordCommands", "privacy.recordCommands"), ("/completion/autoSpecs", "completion.autoSpecs"), ("/updates/check", "updates.check"), ("/popup/ownPermission", "popup.ownPermission")] {
        if let Some(value) = v.pointer(path).filter(|value| !value.is_boolean()) {
            return Err(format!("{name} must be true or false, not {value}"));
        }
    }
    if let Some(size) = v.pointer("/popup/fontSize") {
        if !size.as_f64().is_some_and(|s| (8.0..=32.0).contains(&s)) {
            return Err(format!("popup.fontSize must be a number from 8 to 32, not {size}"));
        }
    }
    Ok(())
}

/// What the shell adapters take from the file, as `NAME=value` lines (only settings that are present).
pub fn env_lines(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(m) = v.pointer("/display/mode").and_then(Value::as_str).filter(|m| matches!(*m, "auto" | "overlay" | "inline")) {
        out.push(format!("GOH_DISPLAY={m}"));
    }
    if let Some(n) = v.pointer("/display/maxSuggestions").and_then(Value::as_u64).filter(|n| (1..=50).contains(n)) {
        out.push(format!("GOH_LIMIT={n}"));
    }
    if let Some(live) = v.pointer("/display/live").and_then(Value::as_bool) {
        out.push(format!("GOH_LIVE={}", if live { "on" } else { "off" }));
    }
    for (path, name) in [("/display/predict", "GOH_PREDICT"), ("/privacy/recordCommands", "GOH_RECORD"), ("/updates/check", "GOH_UPDATE_CHECK"), ("/popup/ownPermission", "GOH_POPUP_OWN_PERMISSION")] {
        if let Some(on) = v.pointer(path).and_then(Value::as_bool) {
            out.push(format!("{name}={}", if on { "on" } else { "off" }));
        }
    }
    out
}

/// Terminal editors tried, in order, when neither `$EDITOR` nor `$VISUAL` is set.
const FALLBACK_EDITORS: [&str; 3] = ["nano", "vim", "vi"];

/// The editor `goh config` runs: `$EDITOR`, then `$VISUAL`, then the first of [`FALLBACK_EDITORS`] on `$PATH`.
fn editor() -> Option<String> {
    let set = ["EDITOR", "VISUAL"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()));
    set.or_else(|| {
        let path = std::env::var_os("PATH")?;
        FALLBACK_EDITORS.iter().find(|e| std::env::split_paths(&path).any(|d| d.join(e).is_file())).map(|e| e.to_string())
    })
}

/// Edit `path` in this terminal and wait for the editor to exit, so the file can be checked right after. The editor
/// runs through the shell, so `nvim -u NONE` or `code -w` work; a desktop app is never opened instead.
pub fn open_in_editor(path: &std::path::Path) -> io::Result<()> {
    let Some(editor) = editor() else { return Err(io::Error::other("no editor found; set $EDITOR (e.g. export EDITOR=nvim)")) };
    let status = Command::new("sh").arg("-c").arg(format!("{editor} \"$1\"")).arg("goh").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("the editor `{editor}` exited with {status}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_valid_and_exports_the_adapter_settings() {
        validate(TEMPLATE).unwrap();
        let v = goh_theme::parse_config(TEMPLATE).unwrap();
        let example: Value = serde_json::from_str(include_str!("../../../docs/examples/config.json")).unwrap();
        assert_eq!(v, example, "the commented template and the schema-checked example say the same");
        assert_eq!(env_lines(&v), ["GOH_DISPLAY=auto", "GOH_LIMIT=8", "GOH_LIVE=on", "GOH_PREDICT=off", "GOH_RECORD=off", "GOH_UPDATE_CHECK=off", "GOH_POPUP_OWN_PERMISSION=off"]);
    }

    #[test]
    fn the_template_is_rebuilt_exactly_from_its_own_settings() {
        assert_eq!(with_comments(&goh_theme::parse_config(TEMPLATE).unwrap()), TEMPLATE);
        assert_eq!(with_comments(&serde_json::json!({})), TEMPLATE, "a missing setting gets its default");
    }

    #[test]
    fn an_older_file_gets_comments_and_keeps_every_setting() {
        let old = r##"{"configVersion":1,"display":{"mode":"inline","maxSuggestions":5},"popup":{"font":"Fira Code"},
            "theme":{"mode":"custom","foreground":"#eeeeee","background":"#111111"},"privacy":{"telemetry":false,
            "networkSpecs":true,"dynamicGenerators":false,"retainCommandHistory":false},"$schema":"x"}"##;
        let new = commented(old).unwrap();
        assert!(new.contains("// Where the suggestion menu is drawn.") && new.contains(r#""mode": "inline","#));
        let v = goh_theme::parse_config(&new).unwrap();
        assert_eq!(v.pointer("/theme/background"), Some(&Value::from("#111111")), "settings the template does not name stay");
        assert_eq!((v.pointer("/$schema"), v.pointer("/display/live")), (Some(&Value::from("x")), Some(&Value::Bool(true))));
        assert_eq!(env_lines(&v)[..2], ["GOH_DISPLAY=inline", "GOH_LIMIT=5"]);

        assert!(commented(&new).is_none(), "a file with comments is left alone");
        assert!(commented("{ nope").is_none() && commented(r#"{"display":{"mode":"popup"}}"#).is_none(), "nor one that does not check out");
    }

    #[test]
    fn the_old_file_is_kept_next_to_the_commented_one() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        std::fs::write(&file, r#"{"display":{"maxSuggestions":5}}"#).unwrap();
        std::fs::write(dir.path().join("config.json.bak"), "older").unwrap();
        assert_eq!(add_comments(&file).unwrap(), Some(dir.path().join("config.json.bak.2")), "an earlier copy is not overwritten");
        assert_eq!(std::fs::read_to_string(dir.path().join("config.json.bak.2")).unwrap(), r#"{"display":{"maxSuggestions":5}}"#);
        assert!(std::fs::read_to_string(&file).unwrap().contains(r#""maxSuggestions": 5,"#));
        assert_eq!(add_comments(&file).unwrap(), None, "only once");
    }

    #[test]
    fn unusable_values_are_named() {
        assert!(validate("{ nope").unwrap_err().contains("not valid JSON"));
        assert!(validate(r#"{"display":{"mode":"popup"}}"#).unwrap_err().contains("display.mode"));
        assert!(validate(r#"{"display":{"maxSuggestions":0}}"#).unwrap_err().contains("maxSuggestions"));
        assert!(validate(r#"{"display":{"live":"yes"}}"#).unwrap_err().contains("display.live"));
        assert!(validate(r#"{"popup":{"fontSize":2}}"#).unwrap_err().contains("fontSize"));
        assert!(validate(r#"{"privacy":{"recordCommands":"yes"}}"#).unwrap_err().contains("privacy.recordCommands"));
        assert!(validate(r#"{"completion":{"autoSpecs":1}}"#).unwrap_err().contains("completion.autoSpecs"));
        assert!(validate(r#"{"display":{"predict":null}}"#).unwrap_err().contains("display.predict"));
        assert!(validate(r#"{"popup":{"ownPermission":"yes"}}"#).unwrap_err().contains("popup.ownPermission"));
        validate("{}").unwrap();
    }

    #[test]
    fn invalid_values_never_reach_the_adapters() {
        let v: Value = serde_json::from_str(r#"{"display":{"mode":"popup","maxSuggestions":999}}"#).unwrap();
        assert!(env_lines(&v).is_empty());
    }

    #[test]
    fn live_false_becomes_off() {
        let v: Value = serde_json::from_str(r#"{"display":{"live":false}}"#).unwrap();
        assert_eq!(env_lines(&v), ["GOH_LIVE=off"]);
    }
}
