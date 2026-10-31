//! adapters/bash/goh.bash under ble.sh, in a bash 4+ on a pseudo-terminal: goh's candidates with their descriptions in
//! ble.sh's menu, opened by itself while typing. Skipped (with a note) without bash 4+, ble.sh or a UTF-8 locale.
//! ble.sh is found through GOH_TEST_BLESH (its ble.sh file) or ~/.local/share/blesh/ble.sh.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::{utf8_locale, Fixture, Pty};

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/bash/goh.bash")
}

/// A bash of version 4 or newer: macOS's /bin/bash is 3.2, Homebrew's is not.
fn bash4() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.map(|d| d.join("bash")).filter(|b| b.is_file()).find(|b| {
        Command::new(b).args(["-c", "echo ${BASH_VERSINFO[0]}"]).output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok())
            .is_some_and(|v| v >= 4)
    })
}

fn blesh() -> Option<PathBuf> {
    std::env::var_os("GOH_TEST_BLESH").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/blesh/ble.sh")))
        .filter(|p| p.is_file())
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

fn shell(fx: &Fixture, bash: &Path, ble: &Path, env: &[(&str, &str)]) -> Pty {
    let mut sh = Pty::spawn(fx.command(bash.to_str().unwrap(), &["--noprofile", "--norc", "-i"], env));
    sh.drain(Duration::from_millis(400), Duration::from_secs(3));
    sh.send(&format!("PS1='READY> '; source '{}' --attach=none\n", ble.display()), 1500);
    sh.send(&format!("source '{}'\n", adapter().display()), 800);
    sh.send("ble-attach\n", 1500);
    sh.drain(Duration::from_millis(800), Duration::from_secs(10)); // the first load fills ble.sh's cache
    sh
}

#[test]
fn the_menu_opens_by_itself_with_descriptions() {
    let (Some(bash), Some(ble), Some(locale)) = (bash4(), blesh(), utf8_locale()) else {
        eprintln!("skipping: needs bash 4+, ble.sh and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = shell(&fx, &bash, &ble, &[("GOH_LIVE", "on")]);
    sh.send("git c", 0);
    let shown = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(shown.contains("checkout"), "the menu lists git's subcommands without Tab: {shown}");
    assert!(shown.contains("Switch branches"), "with goh's descriptions: {shown}");
}

#[test]
fn with_live_off_the_menu_waits_for_tab() {
    let (Some(bash), Some(ble), Some(locale)) = (bash4(), blesh(), utf8_locale()) else {
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = shell(&fx, &bash, &ble, &[("GOH_LIVE", "off")]);
    sh.send("git c", 0);
    let idle = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(!idle.contains("Switch branches"), "no menu before Tab: {idle}");
    sh.send("\t", 0);
    let tabbed = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(tabbed.contains("Switch branches"), "Tab opens it, with descriptions: {tabbed}");
}
