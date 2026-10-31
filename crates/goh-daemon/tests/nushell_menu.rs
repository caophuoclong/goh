//! adapters/nushell/goh.nu in a real nushell on a pseudo-terminal: Space opens goh's box with the next word's
//! candidates. Skipped (with a note) when nu or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::{utf8_locale, Fixture, Pty};

fn nu() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("nu")).find(|f| f.is_file()))
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/nushell/goh.nu")
}

/// What the terminal shows: `out` without CSI and OSC sequences.
fn screen(out: &[u8]) -> String {
    let s = String::from_utf8_lossy(out);
    let (mut text, mut chars) = (String::new(), s.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\x1b', Some('[')) => {
                chars.next();
                while chars.next().is_some_and(|c| !('@'..='~').contains(&c)) {}
            }
            ('\x1b', Some(']')) => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => text.push(c),
        }
    }
    text
}

/// Collect output until it has been quiet for `quiet_ms` (or `max_ms` passed), answering the cursor-position
/// queries (`ESC [ 6 n`) reedline sends and waits on: this pseudo-terminal has no emulator behind it.
fn pump(sh: &mut Pty, quiet_ms: u64, max_ms: u64) -> Vec<u8> {
    let (start, mut last, mut all) = (Instant::now(), Instant::now(), Vec::new());
    while start.elapsed() < Duration::from_millis(max_ms) && last.elapsed() < Duration::from_millis(quiet_ms) {
        let chunk = sh.drain(Duration::from_millis(30), Duration::from_millis(100));
        if !chunk.is_empty() {
            last = Instant::now();
            for _ in chunk.windows(4).filter(|w| *w == b"\x1b[6n") {
                sh.write_all_keys(b"\x1b[1;1R");
            }
            all.extend_from_slice(&chunk);
        }
    }
    all
}

fn type_keys(sh: &mut Pty, keys: &str, quiet_ms: u64) -> Vec<u8> {
    sh.write_all_keys(keys.as_bytes());
    pump(sh, quiet_ms, 4000)
}

#[test]
fn space_opens_a_box_of_goh_candidates() {
    let (Some(nu), Some(locale)) = (nu(), utf8_locale()) else {
        eprintln!("skipping: needs nu and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = Pty::spawn(fx.command(nu.to_str().unwrap(), &["-n"], &[]));
    pump(&mut sh, 800, 5000);
    type_keys(&mut sh, &format!("source '{}'\r", adapter().display()), 800);
    type_keys(&mut sh, "goh-ui\r", 800);
    type_keys(&mut sh, "git", 300);
    let shown = screen(&type_keys(&mut sh, " ", 800));
    assert!(shown.contains("checkout"), "the box lists git's subcommands: {shown}");
    assert!(shown.contains("Switch branches"), "with their descriptions: {shown}");
    assert!(["╭", "┌", "│"].iter().any(|b| shown.contains(b)), "inside a border: {shown}");
}
