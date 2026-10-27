//! What you pick, remembered locally so the same pick comes first next time (`git checkout` -> `main`).
//! One appended line per pick: `epoch TAB cwd TAB key TAB label`. The key is only the command and its
//! subcommand, never the rest of what was typed, so no arguments (tokens, paths) are kept; the label is
//! the suggestion that was chosen. `goh learn` writes it, the daemon reads it; `goh forget` deletes it.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const COMPACT_ABOVE: u64 = 512 * 1024;
const KEEP_LINES: usize = 3000;
const MAX_BOOST: f64 = 5.0;

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_USAGE") {
        return p.into();
    }
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    };
    base.join("goh/usage")
}

/// `git checkout ` -> `git checkout`; `git -C x checkout ` -> `git`; `curl -H "a: b" ` -> `curl`.
/// A first word and a second one that looks like a plain subcommand; anything else is not part of the key.
pub fn key_of(prefix: &str) -> Option<String> {
    let mut words = prefix.split_whitespace();
    let first = words.next()?;
    let plain = |w: &str| w.len() <= 40 && !w.starts_with('-') && w.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !plain(first) {
        return None;
    }
    // The second word only counts when it is the last complete word (a subcommand, not an argument).
    let rest: Vec<&str> = words.collect();
    let ends_with_space = prefix.ends_with(char::is_whitespace);
    Some(match rest.as_slice() {
        [second] if ends_with_space && plain(second) => format!("{first} {second}"),
        _ => first.to_string(),
    })
}

fn clean(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && !s.chars().any(char::is_control)
}

pub fn record(file: &Path, cwd: &str, prefix: &str, label: &str, at: u64) -> std::io::Result<()> {
    let Some(key) = key_of(prefix) else { return Ok(()) };
    if !clean(label) || !clean(cwd) {
        return Ok(());
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(file)?.write_all(format!("{at}\t{cwd}\t{key}\t{label}\n").as_bytes())?;
    if fs::metadata(file).is_ok_and(|m| m.len() > COMPACT_ABOVE) {
        let text = fs::read_to_string(file)?;
        let lines: Vec<&str> = text.lines().collect();
        let tmp = file.with_extension("tmp");
        fs::write(&tmp, lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n")?;
        fs::rename(tmp, file)?;
    }
    Ok(())
}

pub(crate) fn weight(age_secs: u64) -> f64 {
    match age_secs {
        0..=3_599 => 4.0,
        3_600..=86_399 => 2.0,
        86_400..=604_799 => 0.5,
        _ => 0.25,
    }
}

/// Picks, summed with recency weights: per (key, label) overall and per working directory.
#[derive(Default)]
pub struct Usage {
    overall: HashMap<(String, String), f64>,
    here: HashMap<(String, String, String), f64>,
}

impl Usage {
    pub fn load(file: &Path, now: u64) -> Usage {
        let mut u = Usage::default();
        let Ok(text) = fs::read_to_string(file) else { return u };
        for line in text.lines() {
            let mut f = line.splitn(4, '\t');
            let (Some(t), Some(cwd), Some(key), Some(label)) = (f.next(), f.next(), f.next(), f.next()) else { continue };
            let Ok(t) = t.parse::<u64>() else { continue };
            let w = weight(now.saturating_sub(t));
            *u.overall.entry((key.into(), label.into())).or_default() += w;
            *u.here.entry((cwd.into(), key.into(), label.into())).or_default() += w;
        }
        u
    }

    /// How much to raise a suggestion's score: 0 for something never picked, at most [`MAX_BOOST`]. A pick made
    /// in this directory counts double, since the same command means different things in different projects.
    pub fn boost(&self, cwd: &str, prefix: &str, label: &str) -> f64 {
        let Some(key) = key_of(prefix) else { return 0.0 };
        let all = self.overall.get(&(key.clone(), label.to_string())).copied().unwrap_or(0.0);
        let here = self.here.get(&(cwd.to_string(), key, label.to_string())).copied().unwrap_or(0.0);
        ((all + here).ln_1p() * 1.5).min(MAX_BOOST)
    }
}

/// `privacy.learnChoices` in the config file (on unless it says `false`).
pub fn enabled() -> bool {
    goh_theme::read_config().and_then(|c| c.pointer("/privacy/learnChoices").and_then(|v| v.as_bool())).unwrap_or(true)
}

/// Forget everything.
pub fn forget(file: &Path) -> std::io::Result<bool> {
    match fs::remove_file(file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_hold_only_the_command_and_a_plain_subcommand() {
        assert_eq!(key_of("git checkout ").as_deref(), Some("git checkout"));
        assert_eq!(key_of("git ").as_deref(), Some("git"));
        assert_eq!(key_of("git -C repo ").as_deref(), Some("git"), "options and their values are not kept");
        assert_eq!(key_of("curl -H \"Authorization: Bearer x\" ").as_deref(), Some("curl"));
        assert_eq!(key_of("git checkout ma").as_deref(), Some("git"), "a word still being typed is not a subcommand");
        assert_eq!(key_of("FOO=bar make ").as_deref(), None);
        assert_eq!(key_of(""), None);
    }

    #[test]
    fn what_you_picked_here_and_recently_ranks_higher_and_nothing_is_kept_beyond_labels() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data/usage");
        let now = 10_000_000;
        for _ in 0..3 {
            record(&file, "/work/a", "git checkout ", "main", now - 10).unwrap();
        }
        record(&file, "/work/b", "git checkout ", "develop", now - 9_000_000).unwrap();
        record(&file, "/work/a", "curl -H \"secret\" ", "--silent", now).unwrap();
        record(&file, "/work/a", "git checkout ", "bad\nlabel", now).unwrap();
        let u = Usage::load(&file, now);
        let (main, develop, never) = (u.boost("/work/a", "git checkout ", "main"), u.boost("/work/a", "git checkout ", "develop"), u.boost("/work/a", "git checkout ", "x"));
        assert!(main > develop && develop > never && never == 0.0, "{main} {develop} {never}");
        assert!(main <= MAX_BOOST);
        assert!(u.boost("/work/a", "git checkout ", "main") > u.boost("/elsewhere", "git checkout ", "main"), "this directory counts more");
        let kept = fs::read_to_string(&file).unwrap();
        assert!(!kept.contains("secret") && !kept.contains("bad"), "{kept}");
    }

    #[test]
    fn forget_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("usage");
        assert!(!forget(&file).unwrap());
        record(&file, "/a", "git ", "status", 1).unwrap();
        assert!(forget(&file).unwrap() && !file.exists());
    }
}
