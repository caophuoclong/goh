//! Telling you a newer goh exists (`updates.check`, off unless you turn it on). Once a day, in the background, goh
//! asks GitHub for the version number of the latest release and keeps it in a small file; the shell then prints
//! one line, once per new version, when it starts. Nothing about you or what you type is sent: it is a plain
//! request for the public release page, made with `curl`. `goh update` does the same check once, on request,
//! whether or not the setting is on.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// Where the latest release is read from (`GOH_UPDATE_URL` replaces it, e.g. with a `file://` URL in tests).
const RELEASE_URL: &str = "https://api.github.com/repos/caophuoclong/goh/releases/latest";
const EVERY: u64 = 24 * 3600;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// When the last check ran (seconds since the epoch), whether or not it found anything.
    #[serde(default)]
    pub checked: u64,
    /// The newest version seen, without a leading `v`.
    #[serde(default)]
    pub latest: String,
    /// The version the shell was last told about.
    #[serde(default)]
    pub notified: String,
}

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_UPDATE_STATE") {
        return p.into();
    }
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    };
    base.join("goh/update")
}

/// `GOH_UPDATE_CHECK` (`on` / `off`) if set, else `updates.check` in the config file: off unless it says `true`.
pub fn enabled() -> bool {
    match std::env::var("GOH_UPDATE_CHECK").as_deref() {
        Ok("on" | "1" | "true") => true,
        Ok("off" | "0" | "false") => false,
        _ => goh_theme::read_config().and_then(|c| c.pointer("/updates/check").and_then(|v| v.as_bool())).unwrap_or(false),
    }
}

pub fn load() -> State {
    fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(state: &State) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(state) {
        let _ = fs::write(path, text);
    }
}

/// `1.4.2` or `v1.4.2` as numbers. Anything else (a pre-release, a word) is not a version to announce.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.trim().trim_start_matches('v').split('.');
    let v = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(v)
}

/// Is `latest` a higher version than `current`?
pub fn newer(current: &str, latest: &str) -> bool {
    matches!((parse_version(current), parse_version(latest)), (Some(c), Some(l)) if l > c)
}

/// The release tag in the page GitHub returns, without the `v`.
fn tag_of(body: &str) -> Option<String> {
    let tag = serde_json::from_str::<serde_json::Value>(body).ok()?.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    parse_version(&tag).map(|_| tag)
}

/// Ask for the latest version number. `None`: no `curl`, no network, or an answer that is not a release.
pub fn fetch() -> Option<String> {
    let url = std::env::var("GOH_UPDATE_URL").unwrap_or_else(|_| RELEASE_URL.into());
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "8", "-H", "Accept: application/vnd.github+json", "-A", concat!("goh/", env!("CARGO_PKG_VERSION")), &url])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| tag_of(&String::from_utf8_lossy(&out.stdout))).flatten()
}

/// Check now and remember the answer. A failed check still counts as a check, so a missing network is asked
/// about once a day, not at every prompt.
pub fn refresh(now: u64) -> (State, bool) {
    let mut state = load();
    state.checked = now;
    let latest = fetch();
    if let Some(latest) = &latest {
        state.latest = latest.clone();
    }
    save(&state);
    (state, latest.is_some())
}

pub fn due(state: &State, now: u64) -> bool {
    now.saturating_sub(state.checked) >= EVERY
}

/// How to get the new version, from where this `goh` is installed.
pub fn upgrade_hint() -> &'static str {
    let exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    if exe.contains("/Cellar/") || exe.contains("/homebrew/") || exe.contains("/Homebrew/") {
        "brew update && brew upgrade goh"
    } else {
        "curl -fsSL https://raw.githubusercontent.com/caophuoclong/goh/main/install.sh | sh"
    }
}

/// The one line for a new version, if there is one.
pub fn message(current: &str, latest: &str) -> Option<String> {
    newer(current, latest).then(|| {
        format!("goh {latest} is available (you have {current}): {}, then `goh reload`. Changes: https://github.com/caophuoclong/goh/releases", upgrade_hint())
    })
}

/// What the shell prints when it starts. Reads the file only; if the last check is old it starts one in the
/// background, which the next shell then sees. Tells about each version once.
pub fn notice(current: &str, now: u64) -> Option<String> {
    if !enabled() {
        return None;
    }
    let mut state = load();
    if due(&state, now) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = Command::new(exe).args(["update", "--refresh"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        }
    }
    if state.notified == state.latest {
        return None;
    }
    let line = message(current, &state.latest)?;
    state.notified = state.latest.clone();
    save(&state);
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(newer("1.4.1", "1.4.2"));
        assert!(newer("1.4.9", "1.10.0"));
        assert!(newer("1.4.1", "v2.0.0"));
        assert!(!newer("1.4.1", "1.4.1"));
        assert!(!newer("1.4.2", "1.4.1"));
        assert!(!newer("1.4.1", "1.5.0-rc1"), "a pre-release is not announced");
        assert!(!newer("1.4.1", ""), "nothing known yet");
    }

    #[test]
    fn the_tag_is_read_from_the_release_page_and_nothing_else_is_taken() {
        assert_eq!(tag_of(r#"{"tag_name":"v1.4.2","name":"x"}"#).as_deref(), Some("1.4.2"));
        assert_eq!(tag_of(r#"{"tag_name":"nightly"}"#), None);
        assert_eq!(tag_of(r#"{"message":"Not Found"}"#), None);
        assert_eq!(tag_of("<html>"), None);
    }

    #[test]
    fn a_check_is_due_after_a_day() {
        let s = State { checked: 1000, ..Default::default() };
        assert!(!due(&s, 1000 + EVERY - 1));
        assert!(due(&s, 1000 + EVERY));
        assert!(due(&State::default(), EVERY));
    }

    #[test]
    fn the_message_names_both_versions_and_the_upgrade() {
        let m = message("1.4.1", "1.4.2").unwrap();
        assert!(m.contains("1.4.2") && m.contains("1.4.1") && m.contains("goh reload"), "{m}");
        assert_eq!(message("1.4.2", "1.4.2"), None);
    }
}
