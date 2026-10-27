//! Built-in directory history, used when zoxide is not installed. The shell adapters append one
//! `epoch<TAB>path` line per directory change (`goh visit`); frecency is computed when reading.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const COMPACT_ABOVE: u64 = 1 << 20;
const KEEP_LINES: usize = 5000;
const MAX_PATH: usize = 1024;
const MAX_RESULTS: usize = 500;

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_VISITS") {
        return p.into();
    }
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    };
    base.join("goh/dirs")
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn valid(dir: &str) -> bool {
    dir.starts_with('/') && dir.len() <= MAX_PATH && !dir.chars().any(char::is_control)
}

/// Append a visit. Silently ignores paths that are not absolute or that contain control characters.
pub fn record(file: &Path, dir: &str, at: u64) -> std::io::Result<()> {
    if !valid(dir) || std::env::var_os("HOME").is_some_and(|h| h == dir) {
        return Ok(());
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    // One short O_APPEND write: concurrent shells cannot interleave inside a line.
    OpenOptions::new().create(true).append(true).open(file)?.write_all(format!("{at}\t{dir}\n").as_bytes())?;
    if fs::metadata(file).is_ok_and(|m| m.len() > COMPACT_ABOVE) {
        compact(file)?;
    }
    Ok(())
}

fn compact(file: &Path) -> std::io::Result<()> {
    let text = fs::read_to_string(file)?;
    let lines: Vec<&str> = text.lines().collect();
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n")?;
    fs::rename(tmp, file)
}

fn weight(age_secs: u64) -> f64 {
    match age_secs {
        0..=3_599 => 4.0,
        3_600..=86_399 => 2.0,
        86_400..=604_799 => 0.5,
        _ => 0.25,
    }
}

/// Existing directories, best frecency first.
pub fn ranked(file: &Path, at: u64) -> Vec<String> {
    let Ok(text) = fs::read_to_string(file) else { return vec![] };
    let mut scores: std::collections::HashMap<&str, f64> = Default::default();
    for line in text.lines() {
        let Some((t, dir)) = line.split_once('\t') else { continue };
        let Ok(t) = t.parse::<u64>() else { continue };
        if valid(dir) {
            *scores.entry(dir).or_default() += weight(at.saturating_sub(t));
        }
    }
    let mut all: Vec<_> = scores.into_iter().collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    all.into_iter().map(|(d, _)| d).filter(|d| Path::new(d).is_dir()).take(MAX_RESULTS).map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frecency_prefers_recent_and_frequent_and_drops_missing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, gone) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("gone"));
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let file = dir.path().join("data/dirs");
        let now = 10_000_000;
        record(&file, a.to_str().unwrap(), now - 5_000_000).unwrap();
        record(&file, a.to_str().unwrap(), now - 5_000_000).unwrap();
        record(&file, b.to_str().unwrap(), now - 10).unwrap();
        record(&file, gone.to_str().unwrap(), now).unwrap();
        record(&file, "relative", now).unwrap();
        record(&file, "/bad\x07", now).unwrap();
        assert_eq!(ranked(&file, now), [b.to_str().unwrap(), a.to_str().unwrap()]);
        assert!(ranked(&dir.path().join("missing"), now).is_empty());
    }

    #[test]
    fn compaction_keeps_the_newest_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("dirs");
        let lines: String = (0..KEEP_LINES + 10).map(|i| format!("{i}\t/x{i}\n")).collect();
        fs::write(&file, lines).unwrap();
        compact(&file).unwrap();
        let kept = fs::read_to_string(&file).unwrap();
        assert_eq!(kept.lines().count(), KEEP_LINES);
        assert!(kept.starts_with("10\t/x10\n"));
    }
}
