//! zsh adapter + the opt-in command record, in a real zsh on a pseudo-terminal: lines that ran are recorded,
//! the usual next command shows as ghost text at an empty prompt when prediction is on (Right takes it), `$NA` completes from the
//! shell's exported variables. The daemon runs in this process, so the process-wide settings are set once here.

mod common;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::{kill_socket_holder, utf8_locale, Fixture, Pty};

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/zsh/goh.zsh")
}

impl Fixture {
    /// zsh with the adapter loaded, the record and prediction on, and a `^Xd` widget that dumps the line state.
    fn recording_shell(&self, runs: &Path) -> Pty {
        let env = [("GOH_RECORD", "on"), ("GOH_PREDICT", "on"), ("GOH_LIVE", "on"), ("GOH_RUNS", runs.to_str().unwrap())];
        let mut pty = Pty::spawn(self.command("zsh", &["-f", "-i"], &env));
        pty.drain(Duration::from_millis(500), Duration::from_secs(3));
        let dump = self.path("dump.txt");
        pty.send(&format!(
            "PS1='READY> '\ndump() {{ print -r -- \"BUF[$BUFFER] CUR=$CURSOR on=$_goh_on post[${{POSTDISPLAY%%[[:cntrl:]]*}}]\" >> {} }}\nzle -N dump; bindkey '^Xd' dump\n",
            dump.display()
        ), 300);
        pty.send(&format!("source {}\n", adapter().display()), 900);
        pty
    }

    fn dumped(&self, pty: &mut Pty) -> String {
        let file = self.path("dump.txt");
        let before = fs::read_to_string(&file).map(|s| s.lines().count()).unwrap_or(0);
        pty.master.write_all(b"\x18d").unwrap();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if let Ok(s) = fs::read_to_string(&file) {
                if s.lines().count() > before {
                    return s.lines().last().unwrap().to_string();
                }
            }
            pty.drain(Duration::from_millis(50), Duration::from_millis(100));
        }
        panic!("no dump produced");
    }
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(4) {
        if ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn the_record_is_kept_the_next_command_is_predicted_and_variables_complete() {
    let Some(locale) = utf8_locale().filter(|_| Command::new("zsh").arg("--version").output().is_ok()) else {
        eprintln!("skipping: needs zsh and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, false);
    let runs = fx.path("runs");
    std::env::set_var("GOH_RECORD", "on");
    std::env::set_var("GOH_RUNS", &runs);
    let cwd = fs::canonicalize(fx.dir.path()).unwrap();
    let cwd = cwd.to_str().unwrap();
    let now = goh_daemon::visits::now();
    for (ago, line) in [(900, "git add ."), (890, "git commit -m ship"), (400, "git add ."), (390, "git commit -m ship"), (10, "git add .")] {
        goh_daemon::runs::record(&runs, cwd, line, now - ago).unwrap();
    }

    let mut sh = fx.recording_shell(&runs);
    // At the empty prompt the usual next command shows as ghost text; Right takes it into the line.
    let state = fx.dumped(&mut sh);
    assert_eq!(state, "BUF[] CUR=0 on=0 post[git commit -m ship]");
    sh.send("\x1b[C", 300);
    assert_eq!(fx.dumped(&mut sh), "BUF[git commit -m ship] CUR=18 on=0 post[]");
    sh.send("\x15", 100);
    assert!(fx.dumped(&mut sh).starts_with("BUF[] "), "the line is cleared");

    // Typing anything else drops it.
    sh.send("l", 400);
    let state = fx.dumped(&mut sh);
    assert!(state.starts_with("BUF[l]") && !state.contains("git commit"), "{state}");
    sh.send("\x15", 100);

    // A successful command is recorded with its folder; a failed one, and one starting with a space, are not.
    sh.send("print -r -- hello-record\n", 600);
    sh.send("false\n", 600);
    sh.send(" print -r -- hidden-line\n", 600);
    wait_for("the record", || fs::read_to_string(&runs).is_ok_and(|t| t.contains("\tprint -r -- hello-record")));
    let text = fs::read_to_string(&runs).unwrap();
    assert!(text.contains(&format!("\t{cwd}\tprint -r -- hello-record")), "{text}");
    assert!(!text.contains("\tfalse") && !text.contains("hidden-line"), "{text}");

    // `$GOHVAR_T` completes from the exported variables the adapter sent (as you type, inside a line the
    // fixture's git spec recognises, where an option takes a value).
    sh.send("export GOHVAR_TEST=1\n", 800);
    sh.send("git -C $GOHVAR_T", 600);
    let state = fx.dumped(&mut sh);
    assert!(state.contains("on=1") && state.ends_with("post[EST]"), "{state}");
    sh.send("\t", 300);
    assert!(fx.dumped(&mut sh).starts_with("BUF[git -C $GOHVAR_TEST]"));
    kill_socket_holder(&fx.socket);
}
