//! `gohd.log`: what the daemon did, never what you typed (docs/08). It records starts, spec warnings, refused
//! connections and where a thread panicked; no buffer or command line ever reaches it, which is why the panic hook
//! writes the source location and leaves out the message (a slicing panic quotes the string it sliced).
//!
//! One private file (0600), moved aside to `gohd.log.1` at the next start once it passes [`MAX_BYTES`].

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 256 * 1024;
/// A condition that repeats (a flood of refused connections) is written at most this often.
const THROTTLE: Duration = Duration::from_secs(60);

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
static LAST: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_LOG") {
        return p.into();
    }
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"),
    };
    base.join("goh/gohd.log")
}

/// Open the log and route panics into it. Without a log (no writable folder) the daemon runs as before.
pub fn init() {
    let file = path();
    if open(&file).is_err() {
        return;
    }
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map_or_else(|| "unknown location".to_string(), |l| format!("{}:{}", l.file(), l.line()));
        line(&format!("panic at {at}"));
    }));
}

fn open(file: &Path) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(file).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(file, file.with_extension("log.1"));
    }
    let f = OpenOptions::new().create(true).append(true).mode(0o600).open(file)?;
    let _ = FILE.set(Mutex::new(f));
    Ok(())
}

/// Append one line. Never fails the caller.
pub fn line(msg: &str) {
    let Some(file) = FILE.get() else { return };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{} {}", utc(secs), msg.replace('\n', " "));
    }
}

/// Like [`line`], for a condition that can repeat: at most once a minute per `key`.
pub fn throttled(key: &'static str, msg: &str) {
    let Ok(mut last) = LAST.lock() else { return };
    match last.iter_mut().find(|(k, _)| *k == key) {
        Some((_, at)) if at.elapsed() < THROTTLE => return,
        Some((_, at)) => *at = Instant::now(),
        None => last.push((key, Instant::now())),
    }
    drop(last);
    line(msg);
}

/// The last `n` lines of the log at `file`, or `None` when there is none.
pub fn tail(file: &Path, n: usize) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    Some(lines[lines.len().saturating_sub(n)..].join("\n"))
}

/// Seconds since the epoch as `YYYY-MM-DDTHH:MM:SSZ` (days-to-civil, proleptic Gregorian).
fn utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_000_000_000), "2001-09-09T01:46:40Z");
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("log");
        fs::write(&f, "a\nb\nc\nd\n").unwrap();
        assert_eq!(tail(&f, 2).as_deref(), Some("c\nd"));
        assert_eq!(tail(&f, 10).as_deref(), Some("a\nb\nc\nd"));
        assert_eq!(tail(&dir.path().join("none"), 2), None);
    }

    #[test]
    fn a_big_log_is_moved_aside_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("gohd.log");
        fs::write(&f, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        // `FILE` is process-wide, so only the rotation is checked here.
        let _ = open(&f);
        assert!(dir.path().join("gohd.log.1").exists());
        assert_eq!(fs::metadata(&f).map(|m| m.len()).unwrap_or(0), 0);
    }
}
