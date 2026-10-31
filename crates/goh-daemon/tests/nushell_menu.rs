//! adapters/nushell/goh.nu in a real nushell on a pseudo-terminal: Space opens goh's box with the next word's
//! candidates. Skipped (with a note) when nu or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
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

fn shell(mut command: Command) -> Pty {
    let setup = format!(
        "source '{}'; goh-ui; $env.PROMPT_COMMAND = {{|| 'READY> '}}; $env.PROMPT_COMMAND_RIGHT = {{|| ''}}",
        adapter().display(),
    );
    command.args(["-n", "-e", &setup]);
    let mut sh = Pty::spawn(command);
    let start = Instant::now();
    let mut output = Vec::new();
    while start.elapsed() < Duration::from_secs(10) {
        output.extend(pump(&mut sh, 100, 500));
        if screen(&output).contains("READY> ") {
            return sh;
        }
    }
    panic!("Nushell did not load the adapter and reach its prompt: {}", screen(&output));
}

#[test]
fn space_opens_a_box_of_goh_candidates() {
    let (Some(nu), Some(locale)) = (nu(), utf8_locale()) else {
        eprintln!("skipping: needs nu and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = shell(fx.command(nu.to_str().unwrap(), &[], &[]));
    type_keys(&mut sh, "git", 300);
    let shown = screen(&type_keys(&mut sh, " ", 800));
    if !shown.contains("checkout") {
        let script = format!(
            "source '{}'; print $nu.pid; print $env.PATH; print (which goh | to nuon); print (do {{ ^goh menu --live --session $\"nu-($nu.pid)\" --cwd $env.PWD --limit 50 --cursor-byte 4 --buffer 'git ' }} | complete | to nuon); print (goh-menu-source 'git ' 4 | to nuon)",
            adapter().display(),
        );
        let diagnostic = fx.command(nu.to_str().unwrap(), &["-n", "-c", &script], &[]).output().unwrap();
        panic!("the box lists git's subcommands: {shown}\ncompletion diagnostic: {diagnostic:?}");
    }
    assert!(shown.contains("Switch branches"), "with their descriptions: {shown}");
    assert!(["╭", "┌", "│"].iter().any(|b| shown.contains(b)), "inside a border: {shown}");
}

/// `fx.command` with `goh` off PATH: GOH_CTL points at nothing and only the system directories are searched.
fn without_goh(fx: &Fixture, nu: &Path, args: &[&str]) -> std::process::Command {
    let mut cmd = fx.command(nu.to_str().unwrap(), args, &[("GOH_CTL", "/nonexistent/goh")]);
    cmd.env("PATH", "/usr/bin:/bin");
    cmd
}

#[test]
fn menu_source_and_completer_fail_open_without_goh() {
    let (Some(nu), Some(locale)) = (nu(), utf8_locale()) else {
        eprintln!("skipping: needs nu and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, false);
    let script = format!("source '{}'; print (goh-menu-source 'git ' 4 | to nuon); print (goh-complete ['git' ''] | to nuon)", adapter().display());
    let out = without_goh(&fx, &nu, &["-n", "-c", &script]).output().unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "no error escapes: {stderr}");
    assert_eq!(stdout.lines().map(str::trim).collect::<Vec<_>>(), ["[]", "null"], "nothing offered: {stdout}");
}

#[test]
fn space_inserts_a_space_without_goh() {
    let (Some(nu), Some(locale)) = (nu(), utf8_locale()) else {
        eprintln!("skipping: needs nu and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, false);
    let mut sh = shell(without_goh(&fx, &nu, &[]));
    type_keys(&mut sh, "echo", 300);
    let shown = screen(&type_keys(&mut sh, " ", 800));
    assert!(!shown.contains("Error") && !shown.contains("External command failed"), "no error on Space: {shown}");
    let shown = screen(&type_keys(&mut sh, "fine\r", 800));
    assert!(shown.contains("echo fine") && shown.matches("fine").count() >= 2, "the space was typed and the line ran: {shown}");
}
