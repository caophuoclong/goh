//! adapters/bash/goh.bash in a real bash on a pseudo-terminal (whatever `bash` is first on PATH,
//! including macOS's 3.2). Candidates arrive through bash's own programmable completion, so Tab
//! and the candidate list are native; we check what ends up on the line by running it.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::{kill_socket_holder, utf8_locale, Fixture, Pty};

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/bash/goh.bash")
}

fn bash(fx: &Fixture) -> Pty {
    let mut sh = Pty::spawn(fx.command("bash", &["--noprofile", "--norc", "-i"], &[]));
    sh.drain(Duration::from_millis(400), Duration::from_secs(3));
    sh.send("PS1='READY> '; git() { echo \"RAN:$*\"; }\n", 300);
    sh.send(&format!("source {}\n", adapter().display()), 500);
    sh
}

macro_rules! require_env {
    () => {
        match (Command::new("bash").arg("--version").output().is_ok(), utf8_locale()) {
            (true, Some(l)) => l,
            _ => {
                eprintln!("skipping: needs bash and a UTF-8 locale");
                return;
            }
        }
    };
}

/// Complete with Tab, then run the line; returns what the command printed.
fn tab_then_run(sh: &mut Pty, typed: &str, tabs: usize) -> String {
    sh.send(typed, 150);
    for _ in 0..tabs {
        sh.send("\t", 500);
    }
    sh.send("\r", 500)
}

#[test]
fn completes_through_native_tab() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = bash(&fx);

    // Unique candidate: bash inserts it (with our trailing space), nothing is run by Tab itself.
    assert!(tab_then_run(&mut sh, "git chec", 1).contains("RAN:checkout"));

    // Ambiguous: the native list shows our candidates; the line is left for the user to finish.
    sh.send("git c", 150);
    sh.send("\t", 400);
    let list = sh.send("\t", 600);
    for name in ["checkout", "cherry-pick", "commit"] {
        assert!(list.contains(name), "native list should show {name}: {list:?}");
    }
    sh.send("\x15", 100);

    // Multibyte text before the word: byte offsets are computed by the adapter.
    assert!(tab_then_run(&mut sh, "git -C Tài chec", 1).contains("RAN:-C Tài checkout"));
}

#[test]
fn fails_open_to_native_filename_completion() {
    let fx = Fixture::new(require_env!(), true);
    fs::write(fx.path("zzfile"), "").unwrap();
    let mut sh = bash(&fx);
    // No candidate matches `zzf`, so the word is handed back to bash's own file completion.
    assert!(tab_then_run(&mut sh, "git zzf", 1).contains("RAN:zzfile"));
}

#[test]
fn works_without_a_daemon_and_starts_one() {
    let fx = Fixture::new(require_env!(), false);
    fs::write(fx.path("zzfile"), "").unwrap();
    let mut sh = bash(&fx);
    // No daemon yet: the first Tab must not break the line or hang; file completion still works.
    assert!(tab_then_run(&mut sh, "git zzf", 1).contains("RAN:zzfile"));
    let t = std::time::Instant::now();
    while !fx.socket.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(fx.socket.exists(), "gohd should have been started");
    assert!(tab_then_run(&mut sh, "git chec", 1).contains("RAN:checkout"));
    kill_socket_holder(&fx.socket);
}

#[test]
fn bundled_commands_register_and_complete_without_user_specs() {
    let fx = Fixture::new(require_env!(), false);
    fs::remove_file(fx.path("specs/git.json")).unwrap();
    let mut sh = bash(&fx);
    sh.send("pnpm() { echo \"RAN:$*\"; }; complete -p pnpm > bundled.txt\n", 300);
    assert!(fs::read_to_string(fx.path("bundled.txt")).unwrap().contains("_goh_complete"));
    let out = tab_then_run(&mut sh, "pnpm add --save-d", 1);
    assert!(out.contains("RAN:add --save-dev"), "{out:?}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn unload_removes_only_what_it_registered() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = bash(&fx);
    sh.send("complete -p git > unload.txt; goh-unload; complete -p git >> unload.txt 2>&1\n", 400);
    let txt = fs::read_to_string(fx.path("unload.txt")).unwrap();
    assert!(txt.contains("_goh_complete") && txt.contains("no completion specification"), "{txt}");
}
