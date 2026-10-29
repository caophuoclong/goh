//! The settings file (`goh config`): where it is, a first-run template, what the adapters read from it, and
//! opening it in an editor. Environment variables still win over the file, so one-off overrides keep working.

use std::io;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// Written the first time `goh config` runs. It is the schema-validated example, so it stays valid.
pub const TEMPLATE: &str = include_str!("../../../docs/examples/config.json");

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

/// `Err` carries a message fit to show: the syntax error, or the value that cannot be used.
pub fn validate(text: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
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

/// Open `path` in the user's editor: `$VISUAL`, then `$EDITOR` (run through the shell, so `code -w` works), then the
/// system's default text editor. `Ok(true)`: the editor has exited (so the file can be checked now).
pub fn open_in_editor(path: &std::path::Path) -> io::Result<bool> {
    let editor = ["VISUAL", "EDITOR"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()));
    if let Some(editor) = editor {
        let status = Command::new("sh").arg("-c").arg(format!("{editor} \"$1\"")).arg("goh").arg(path).status()?;
        return if status.success() { Ok(true) } else { Err(io::Error::other(format!("the editor `{editor}` exited with {status}"))) };
    }
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg("-t");
        c
    } else {
        Command::new("xdg-open")
    };
    if cmd.arg(path).status()?.success() {
        Ok(false)
    } else {
        Err(io::Error::other("could not open the default editor; set $EDITOR"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_valid_and_exports_the_adapter_settings() {
        validate(TEMPLATE).unwrap();
        let v: Value = serde_json::from_str(TEMPLATE).unwrap();
        assert_eq!(env_lines(&v), ["GOH_DISPLAY=auto", "GOH_LIMIT=8", "GOH_LIVE=on", "GOH_PREDICT=on", "GOH_RECORD=off", "GOH_UPDATE_CHECK=off", "GOH_POPUP_OWN_PERMISSION=off"]);
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
