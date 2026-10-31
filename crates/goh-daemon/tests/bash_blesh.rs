//! adapters/bash/goh.bash under ble.sh, in a bash 4+ on a pseudo-terminal: goh's candidates with their descriptions in
//! ble.sh's menu, opened by itself while typing. Skipped (with a note) without bash 4+, ble.sh or a UTF-8 locale.
//! ble.sh is found through GOH_TEST_BLESH (its ble.sh file) or ~/.local/share/blesh/ble.sh.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::{kill_socket_holder, utf8_locale, Fixture, Pty};

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
    // Without an existing XDG cache directory, ble.sh writes beside its shared installation.
    fs::create_dir(fx.path(".cache")).unwrap();
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

/// Names with a space, and with non-ASCII text, go through the adapter's unescaping and ble.sh's quoting and packing
/// (which work in the user's locale) and come out as one word, with their descriptions. The fixture's in-process
/// daemon only knows `git`, so this one has gohd started on demand by goh, with a `pick` spec of fixed suggestions.
/// They are not files: ble.sh's own file name completion (the `-o default` fallback) cannot supply them if the
/// yield to ble.sh breaks. (`cd` is no use: ble.sh completes it with its own directory source, never asking goh.)
#[test]
fn names_with_a_space_or_non_ascii_text_arrive_as_one_word() {
    let (Some(bash), Some(ble), Some(locale)) = (bash4(), blesh(), utf8_locale()) else {
        return;
    };
    let fx = Fixture::new(locale, false);
    fs::write(
        fx.path("specs/pick.json"),
        r#"{"schemaVersion":1,"name":"pick","args":[{"name":"item","suggestions":[
            {"name":"My Projects","description":"Work in progress"},{"name":"Tài liệu","description":"Giấy tờ"}]}]}"#,
    )
    .unwrap();
    let mut sh = shell(&fx, &bash, &ble, &[("GOH_LIVE", "off")]);
    sh.send("pick() { printf 'ARG<%s>\\n' \"$@\"; }\n", 300);
    // The first call starts the daemon and offers nothing; wait for it, then let later Tabs ask it.
    sh.send("pick \t", 0);
    let t = std::time::Instant::now();
    while !fx.socket.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    sh.drain(Duration::from_millis(1500), Duration::from_secs(5));
    sh.send("\x15", 300);

    sh.send("pick ", 200);
    let menu = screen(sh.send("\t", 1200).as_bytes());
    assert!(menu.contains("Work in progress") && menu.contains("Giấy tờ"), "both descriptions, intact: {menu:?}");
    sh.send("\x15", 300);

    // Picking from the menu goes through ble.sh's unpacking of the candidate; typing a prefix and pressing Tab does not.
    for (selected, word) in [(1, "My Projects"), (2, "Tài liệu")] {
        sh.send("pick ", 200);
        sh.send("\t", 1200); // the menu opens
        for _ in 0..selected {
            sh.send("\t", 300); // the next candidate
        }
        sh.send("\r", 500); // accept it
        let ran = screen(sh.send("\r", 800).as_bytes());
        assert!(ran.contains(&format!("ARG<{word}>")), "menu candidate {selected} should run pick with one argument {word:?}: {ran:?}");
    }
    // A typed prefix and Tab inserts the candidate directly.
    for (typed, word) in [("pick M", "My Projects"), ("pick T", "Tài liệu")] {
        sh.send(typed, 200);
        sh.send("\t", 1200);
        let ran = screen(sh.send("\r", 800).as_bytes());
        assert!(ran.contains(&format!("ARG<{word}>")), "{typed}<Tab> should run pick with one argument {word:?}: {ran:?}");
    }
    kill_socket_holder(&fx.socket);
}
