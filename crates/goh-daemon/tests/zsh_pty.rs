//! End-to-end checks of adapters/zsh/goh.zsh in a real zsh on a pseudo-terminal, with the real
//! gohd/goh-ctl. The test plays the terminal: it answers OSC colour queries on request.
//! Skipped (with a note) when zsh or a UTF-8 locale is missing.

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

fn have_zsh() -> bool {
    Command::new("zsh").arg("--version").output().is_ok()
}

macro_rules! require_env {
    () => {
        match (have_zsh(), utf8_locale()) {
            (true, Some(l)) => l,
            _ => {
                eprintln!("skipping: needs zsh and a UTF-8 locale");
                return;
            }
        }
    };
}

// ---- zsh helpers -------------------------------------------------------------------------------

impl Fixture {
    /// An interactive zsh with a recognisable prompt and `^Xd` / `^Xk` / `^Xj` inspection widgets.
    fn shell(&self, extra_env: &[(&str, &str)], load_adapter: bool) -> Pty {
        let mut pty = Pty::spawn(self.command("zsh", &["-f", "-i"], extra_env));
        pty.drain(Duration::from_millis(500), Duration::from_secs(3));
        let d = self.path("dump.txt");
        let setup = format!(
            "PS1='READY> '\n\
             dump() {{ print -r -- \"BUF[$BUFFER] CUR=$CURSOR on=$_goh_on sel=$_goh_sel post[$POSTDISPLAY]\" >> {d} }}\n\
             keys0() {{ bindkey -L > {k0} }}; keys1() {{ bindkey -L > {k1} }}\n\
             zle -N dump; zle -N keys0; zle -N keys1; bindkey '^Xd' dump; bindkey '^X0' keys0; bindkey '^X1' keys1\n",
            d = d.display(),
            k0 = self.path("keys0.txt").display(),
            k1 = self.path("keys1.txt").display(),
        );
        pty.send(&setup, 300);
        if load_adapter {
            pty.send(&format!("source {}\n", adapter().display()), 700);
        }
        pty
    }

    /// Ask the shell to dump its line state; returns the newest dump line.
    fn state(&self, pty: &mut Pty) -> String {
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

    fn keymap(&self, pty: &mut Pty, slot: u8) -> String {
        let f = self.path(&format!("keys{slot}.txt"));
        let _ = fs::remove_file(&f);
        pty.master.write_all(format!("\x18{slot}").as_bytes()).unwrap();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if let Ok(s) = fs::read_to_string(&f) {
                if !s.is_empty() {
                    std::thread::sleep(Duration::from_millis(50));
                    return fs::read_to_string(&f).unwrap();
                }
            }
            pty.drain(Duration::from_millis(50), Duration::from_millis(100));
        }
        panic!("no keymap dump");
    }
}

const CLEAR: &str = "\x15";

// ---- keyboard flow ---------------------------------------------------------------------------

#[test]
fn keyboard_flow() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[], true);
    let keymap_before = fx.keymap(&mut sh, 0);

    sh.send("git c", 150);
    let drawn = sh.send("\t", 500);
    for needle in ["❯ checkout", "cherry-pick", "commit", "Tab insert"] {
        assert!(drawn.contains(needle), "menu should show {needle:?}: {drawn:?}");
    }
    assert_eq!(fx.state(&mut sh), "BUF[git c] CUR=5 on=1 sel=1 post[heckout ]");

    sh.send("\x1b[B", 200);
    assert!(fx.state(&mut sh).contains("on=1 sel=2 post[herry-pick ]"));
    sh.send("\x1b[B", 200);
    sh.send("\x1b[B", 200);
    assert!(fx.state(&mut sh).contains("sel=1"), "Down wraps to the first");
    sh.send("\x1b[A", 200);
    assert!(fx.state(&mut sh).contains("sel=3"), "Up wraps to the last");
    sh.send("\x1b[Z", 200);
    assert!(fx.state(&mut sh).contains("sel=2"), "Shift+Tab moves back");

    sh.send("\x1b", 900); // lone Esc waits for KEYTIMEOUT
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git c] CUR=5 on=0") && s.ends_with("post[]"), "Esc closes without editing: {s}");
    assert_eq!(fx.keymap(&mut sh, 1), keymap_before, "keymap is restored after the menu closes");

    // Tab accepts the highlighted candidate; it never runs the command.
    sh.send("\t", 500);
    sh.send("\x1b[B", 200);
    sh.send("\t", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git cherry-pick ] CUR=16 on=0"));
    sh.send(CLEAR, 100);

    // Any edit closes the menu and removes the ghost text.
    sh.send("git c", 150);
    sh.send("\t", 500);
    sh.send("h", 300);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git ch] CUR=6 on=0") && s.ends_with("post[]"), "{s}");
    assert_eq!(fx.keymap(&mut sh, 1), keymap_before);
    sh.send(CLEAR, 100);

    // No spec for the command: native completion is left alone.
    sh.send("echo hi", 150);
    sh.send("\t", 600);
    assert!(fx.state(&mut sh).starts_with("BUF[echo hi] CUR=7 on=0"));
    sh.send(CLEAR, 100);

    // Enter keeps the shell's default: the line runs as typed and nothing is inserted.
    sh.send("git() { print -r -- RAN:$*; }\n", 200);
    sh.send("git c", 150);
    sh.send("\t", 500);
    let out = sh.send("\r", 500);
    assert!(out.contains("RAN:c") && !out.contains("checkout"), "line ran as typed: {out:?}");
}

#[test]
fn midline_and_multibyte_edits_use_character_cursors() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[], true);

    // Cursor in the middle: text after it survives, no ghost text.
    sh.send("git -C Tài c xyz", 200);
    sh.send(&"\x02".repeat(4), 150);
    sh.send("\t", 500);
    assert_eq!(fx.state(&mut sh), "BUF[git -C Tài c xyz] CUR=12 on=1 sel=1 post[]");
    sh.send("\x1b[B\t", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git -C Tài cherry-pick  xyz] CUR=23 on=0 sel=2 post[]");
    sh.send(CLEAR, 100);

    // Multibyte before the edit point: byte offsets on the wire, character cursor in zsh.
    sh.send("git -C Tài c", 200);
    sh.send("\t", 500);
    assert!(fx.state(&mut sh).ends_with("post[heckout ]"));
    sh.send("\x1b[B\t", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git -C Tài cherry-pick ] CUR=23 on=0 sel=2 post[]");
}

#[test]
fn cold_start_starts_the_daemon_and_still_completes() {
    let fx = Fixture::new(require_env!(), false);
    let mut sh = fx.shell(&[], true);
    // No daemon yet: the first Tab starts it, waits for it, and opens the menu (no lost request).
    sh.send("git c", 150);
    sh.send("\t", 1500);
    let s = fx.state(&mut sh);
    assert!(s.contains("on=1 sel=1 post[heckout ]"), "first Tab after a cold start completes: {s}");
    assert!(fx.socket.exists(), "gohd should have been started");
    kill_socket_holder(&fx.socket);
}

#[test]
fn a_daemon_that_cannot_start_fails_open_quickly() {
    let fx = Fixture::new(require_env!(), false);
    // The socket directory cannot be created, so gohd exits at once.
    let mut sh = fx.shell(&[("GOH_SOCKET", "/dev/null/nope/s")], true);
    sh.send("git c", 150);
    let t = Instant::now();
    sh.send("\t", 1200);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git c] CUR=5 on=0"), "Tab leaves the line alone: {s}");
    assert!(t.elapsed() < Duration::from_secs(4), "bounded wait: {:?}", t.elapsed());
    sh.send("\x15", 100); // clear the line, then prove the shell still works
    let out = sh.send("print -r -- ALIVE-$((6*7))\n", 500);
    assert!(out.contains("ALIVE-42"), "shell still works: {out:?}");
}

// ---- theme: OSC scenarios --------------------------------------------------------------------

struct Scenario {
    name: &'static str,
    reply: &'static [u8],
    env: &'static [(&'static str, &'static str)],
    source: &'static str,
    osc: &'static str,
    queried: bool,
    adjusted: Option<&'static str>,
}

const SCENARIOS: &[Scenario] = &[
    Scenario { name: "both, 16-bit, ST", reply: b"\x1b]10;rgb:7b7b/8989/8585\x1b\\\x1b]11;rgb:2a2a/3232/3030\x1b\\", env: &[], source: "verified", osc: "verified", queried: true, adjusted: Some("text-primary text-muted") },
    Scenario { name: "both, 8-bit, BEL", reply: b"\x1b]10;rgb:c9/e8/d2\x07\x1b]11;rgb:0d/12/10\x07", env: &[], source: "verified", osc: "verified", queried: true, adjusted: Some("text-muted") },
    Scenario { name: "background only", reply: b"\x1b]11;#f5f0e1\x07", env: &[], source: "approximate", osc: "unknown", queried: true, adjusted: Some("text-muted") },
    Scenario { name: "no answer", reply: b"", env: &[], source: "fallback", osc: "unavailable", queried: true, adjusted: None },
    Scenario { name: "junk answer", reply: b"\x1b]10;rgb:zz/00/00\x07\x1b[31mjunk\x1b]11;rgb:12\x07", env: &[], source: "fallback", osc: "unavailable", queried: true, adjusted: None },
    Scenario { name: "no answer, COLORFGBG hint", reply: b"", env: &[("COLORFGBG", "0;15")], source: "env-hint", osc: "unavailable", queried: true, adjusted: None },
    Scenario { name: "inside tmux: no query sent", reply: b"", env: &[("TMUX", "/tmp/x,1,0")], source: "fallback", osc: "unknown", queried: false, adjusted: None },
];

#[test]
fn osc_scenarios_resolve_theme_without_polluting_the_prompt() {
    let locale = require_env!();
    for sc in SCENARIOS {
        let fx = Fixture::new(locale.clone(), true);
        let mut pty = Pty::spawn(fx.command("zsh", &["-f", "-i"], sc.env));
        let reply = sc.reply.to_vec();
        pty.responder = Some(Box::new(move |_| (!reply.is_empty()).then(|| reply.clone())));
        pty.drain(Duration::from_millis(500), Duration::from_secs(3));
        let d = fx.path("dump.txt");
        pty.send(&format!("PS1='READY> '; dump() {{ print -r -- \"BUF[$BUFFER] CUR=$CURSOR on=$_goh_on sel=$_goh_sel post[$POSTDISPLAY]\" >> {} }}; zle -N dump; bindkey '^Xd' dump\n", d.display()), 300);
        pty.send(&format!("source {}\n", adapter().display()), 900);
        pty.send(&format!("goh-theme > {}\n", fx.path("theme.txt").display()), 400);

        let theme = fs::read_to_string(fx.path("theme.txt")).unwrap();
        let field = |k: &str| theme.lines().find_map(|l| l.strip_prefix(&format!("{k:<17} "))).map(str::to_string);
        assert_eq!(pty.queried, sc.queried, "{}: query sent", sc.name);
        assert_eq!(field("source").as_deref(), Some(sc.source), "{}: source\n{theme}", sc.name);
        assert_eq!(field("osc").as_deref(), Some(sc.osc), "{}: osc", sc.name);
        assert_eq!(field("adjusted").as_deref(), sc.adjusted, "{}: adjusted", sc.name);

        // The terminal's reply must not have leaked into the editable line.
        pty.send("git c", 200);
        assert_eq!(fx.state(&mut pty), "BUF[git c] CUR=5 on=0 sel=1 post[]", "{}: line stays clean", sc.name);

        // Ghost text is drawn in the resolved muted colour, over exactly the ghost characters.
        pty.send(CLEAR, 100);
        pty.send("git c", 200);
        let drawn = pty.send("\t", 700);
        let start = drawn.rfind("\x1b[38;5;").unwrap_or_else(|| panic!("{}: coloured ghost text in {drawn:?}", sc.name));
        assert!(drawn[start..].contains("mheckout \x1b[39m"), "{}: colour spans exactly the ghost: {:?}", sc.name, &drawn[start..]);
    }
}

// ---- overlay: the adapter drives goh-popup ---------------------------------------------------

fn fake_popup() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-popup.sh")
}

impl Fixture {
    /// Like `state`, for any dump file written by a widget bound to `keys`.
    fn state_file(&self, pty: &mut Pty, keys: &str, file: &str) -> String {
        let path = self.path(file);
        let before = fs::read_to_string(&path).map(|s| s.lines().count()).unwrap_or(0);
        pty.master.write_all(keys.as_bytes()).unwrap();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if let Ok(s) = fs::read_to_string(&path) {
                if s.lines().count() > before {
                    return s.lines().last().unwrap().to_string();
                }
            }
            pty.drain(Duration::from_millis(50), Duration::from_millis(100));
        }
        panic!("no dump in {file}");
    }
}

/// A shell with the adapter loaded and the fake popup standing in for goh-popup.
fn overlay_shell(fx: &Fixture, popup: &str, answer: &str, display: &str) -> (Pty, PathBuf) {
    let log = fx.path("popup.log");
    let envs = [("GOH_DISPLAY", display), ("GOH_POPUP", popup), ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", answer)];
    let mut sh = fx.shell(&envs, true);
    let file = fx.path("ov.txt");
    sh.send(
        &format!("dumpov() {{ print -r -- \"ov=$_goh_overlay on=$_goh_on sel=$_goh_sel\" >> {} }}; zle -N dumpov; bindkey '^Xo' dumpov\n", file.display()),
        300,
    );
    (sh, log)
}

fn frames(log: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Poll the popup's log until `ok` holds for the frames received so far.
fn wait_frames(sh: &mut Pty, log: &Path, ok: impl Fn(&[serde_json::Value]) -> bool) -> Vec<serde_json::Value> {
    let t = Instant::now();
    loop {
        let f = frames(log);
        if ok(&f) || t.elapsed() > Duration::from_secs(4) {
            return f;
        }
        sh.drain(Duration::from_millis(50), Duration::from_millis(100));
    }
}

fn selected(f: &serde_json::Value) -> i64 {
    f["state"]["selected"].as_i64().unwrap()
}

#[test]
fn overlay_frames_follow_the_menu() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "auto");

    sh.send("git c", 150);
    sh.send("\t", 800);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    let show = f.iter().find(|v| v["type"] == "show").expect("a show frame");
    assert_eq!((show["anchor"].as_str(), show["mode"].as_str()), (Some("auto"), Some("auto")));
    let items = show["state"]["items"].as_array().unwrap();
    let labels: Vec<_> = items.iter().map(|i| i["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["checkout", "cherry-pick", "commit"]);
    assert_eq!(items[0]["match_ranges"], serde_json::json!([[0, 1]]), "per-item match ranges reach the popup");
    assert_eq!((selected(show), show["state"]["context"].as_str()), (0, Some("git")));
    assert!(show["theme"]["surface-base"].is_string(), "resolved theme tokens are sent");

    // The popup answered "overlay", so the adapter knows it is drawing.
    assert_eq!(fx.state_file(&mut sh, "\x18o", "ov.txt"), "ov=1 on=1 sel=1");

    // Down moves the selection in the popup too.
    sh.send("\x1b[B", 300);
    let f = wait_frames(&mut sh, &log, |f| f.iter().filter(|v| v["type"] == "show").count() >= 2);
    assert_eq!(selected(f.iter().rfind(|v| v["type"] == "show").unwrap()), 1);

    // Esc and edits hide it.
    sh.send("\x1b", 900);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "hide"));
    assert!(f.iter().any(|v| v["type"] == "hide"));

    // Accepting hides it as well, and inserts the highlighted candidate.
    let hides_before = f.iter().filter(|v| v["type"] == "hide").count();
    sh.send("\t", 800);
    sh.send("\x1b[B", 300);
    sh.send("\t", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git cherry-pick ] CUR=16"));
    let f = wait_frames(&mut sh, &log, |f| f.iter().filter(|v| v["type"] == "hide").count() > hides_before);
    assert!(f.iter().filter(|v| v["type"] == "hide").count() > hides_before);
}

#[test]
fn an_inline_decision_keeps_the_inline_menu() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "inline", "auto");
    sh.send("git c", 150);
    let drawn = sh.send("\t", 900);
    wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    assert!(drawn.contains("❯ checkout"), "inline menu is drawn: {drawn:?}");
    assert_eq!(fx.state_file(&mut sh, "\x18o", "ov.txt"), "ov=0 on=1 sel=1");
}

#[test]
fn display_inline_never_starts_the_popup() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "inline");
    sh.send("git c", 150);
    let drawn = sh.send("\t", 900);
    assert!(drawn.contains("❯ checkout"));
    sh.send("\x1b[B", 300);
    assert!(!log.exists(), "no popup process should have been started");
}

#[test]
fn a_dead_popup_fails_open_and_never_blocks_the_shell() {
    let fx = Fixture::new(require_env!(), true);
    // /usr/bin/false exits at once: frames go nowhere, decisions never come.
    let (mut sh, _log) = overlay_shell(&fx, "/usr/bin/false", "overlay", "auto");
    sh.send("git c", 150);
    let drawn = sh.send("\t", 900);
    assert!(drawn.contains("❯ checkout"), "inline menu still appears: {drawn:?}");
    // Far more frames than a FIFO buffers (each is about 1 KB): writing blindly would block the shell.
    sh.send(&"\x1b[B".repeat(400), 1500);
    sh.send("\t", 400);
    assert!(fx.state(&mut sh).contains("BUF[git c"), "line editing still works");
    sh.send("\x15", 100);
    let out = sh.send("print -r -- ALIVE-$((6*7))\n", 500);
    assert!(out.contains("ALIVE-42"), "shell is alive: {out:?}");
}

#[test]
fn leaving_the_shell_tells_the_popup_to_quit() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "auto");
    sh.send("git c", 150);
    sh.send("\t", 800);
    wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    sh.send("\x15", 100);
    sh.send("exit\n", 300);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "quit"));
    assert!(f.iter().any(|v| v["type"] == "quit"), "popup got a quit frame: {f:?}");
}

#[test]
fn goh_status_names_each_layer() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[], true);
    let file = fx.path("status.txt");
    sh.send(&format!("goh-status > {} 2>&1\n", file.display()), 1500);
    let out = fs::read_to_string(&file).unwrap();
    for needle in [
        "adapter    Tab is bound to: \"^I\" _goh_tab",
        "goh-ctl    /",
        "gohd       socket ",
        "specs      ",
        "query      'git c' -> 3 candidate(s): checkout, cherry-pick, commit",
        "theme      source ",
        "last popup decision: none yet",
        "last frame sent:     none yet",
    ] {
        assert!(out.contains(needle), "goh-status should contain {needle:?}:\n{out}");
    }
}

// ---- as-you-type (GOH_LIVE=on) ---------------------------------------------------------------

const LIVE: &[(&str, &str)] = &[("GOH_LIVE", "on")];

#[test]
fn live_menu_opens_while_typing_and_follows_the_line() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);

    // No Tab: typing alone opens it.
    sh.send("git c", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git c] CUR=5 on=1 sel=1 post[heckout ]");

    // One key at a time: each keystroke updates the candidates and the ghost text.
    sh.send("h", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git ch] CUR=6 on=1 sel=1 post[eckout ]");
    sh.send("ec", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git chec] CUR=8 on=1 sel=1 post[kout ]", "only checkout still matches");

    // No candidates: it closes. Deleting back brings it back.
    sh.send("z", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git checz] CUR=9 on=0"));
    sh.send("\x7f", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git chec] CUR=8 on=1 sel=1 post[kout ]");
}

#[test]
fn live_menu_is_only_for_recognised_commands_in_progress() {
    let fx = Fixture::new(require_env!(), true);
    // A file starting with `c`: a non-live query for `zzz c` would offer it, so this catches a lost --live.
    fs::write(fx.path("cfile"), "").unwrap();
    let mut sh = fx.shell(LIVE, true);
    for (typed, open) in [("zzz c", false), ("git foo c", false), ("gi", false), ("git ", true)] {
        sh.send(typed, 400);
        let s = fx.state(&mut sh);
        assert_eq!(s.contains("on=1"), open, "{typed:?}: {s}");
        sh.send("\x15", 200);
    }
    // Tab still completes an unknown command's files: validity only governs the automatic menu.
    fs::write(fx.path("notes.txt"), "").unwrap();
    sh.send("cat no", 300);
    assert!(fx.state(&mut sh).contains("on=0"));
    sh.send("\t", 800);
    assert!(fx.state(&mut sh).starts_with("BUF[cat notes.txt"), "Tab completes files");
}

#[test]
fn esc_hides_it_until_the_line_changes_and_accept_does_not_reopen_it() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send("git c", 400);
    assert!(fx.state(&mut sh).contains("on=1"));
    sh.send("\x1b", 900);
    sh.send("", 500);
    assert!(fx.state(&mut sh).contains("on=0"), "Esc closes it and it stays closed");
    sh.send("h", 400);
    assert!(fx.state(&mut sh).contains("on=1"), "the next keystroke brings it back");

    // Accepting inserts the highlighted candidate; nothing reopens until the next keystroke.
    sh.send("\x1b[B", 300);
    sh.send("\t", 500);
    sh.send("", 500);
    assert!(fx.state(&mut sh).starts_with("BUF[git cherry-pick ] CUR=16 on=0"));
}

#[test]
fn history_recall_does_not_open_the_menu() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send("git() { :; }\n", 300);
    sh.send("git checkout\n", 400);
    sh.send("\x1b[A", 500); // Up: the line becomes `git checkout`, but the user did not type it
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git checkout]") && s.contains("on=0"), "{s}");
}

#[test]
fn live_updates_reach_the_popup_and_an_invalid_line_hides_it() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()), ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "overlay")];
    let mut sh = fx.shell(&envs, true);

    sh.send("git c", 500);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    let labels = |v: &serde_json::Value| -> Vec<String> {
        v["state"]["items"].as_array().unwrap().iter().map(|i| i["label"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(labels(f.iter().find(|v| v["type"] == "show").unwrap()), ["checkout", "cherry-pick", "commit"]);

    // The next keystroke sends the narrower list to the same popup.
    sh.send("h", 500);
    let f = wait_frames(&mut sh, &log, |f| f.iter().filter(|v| v["type"] == "show").count() >= 2);
    assert_eq!(labels(f.iter().rfind(|v| v["type"] == "show").unwrap()), ["checkout", "cherry-pick"]);

    // Typing something the spec does not know hides it.
    sh.send("zz", 500);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "hide"));
    assert!(f.iter().any(|v| v["type"] == "hide"), "frames: {f:?}");
}

#[test]
fn live_off_keeps_the_menu_tab_triggered() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "off")], true);
    sh.send("git c", 400);
    assert!(fx.state(&mut sh).contains("on=0"), "typing alone does nothing");
    sh.send("\t", 800);
    assert!(fx.state(&mut sh).contains("on=1"), "Tab opens it");
}

#[test]
fn a_finished_word_does_not_open_a_menu_but_does_not_blink_an_open_one_away() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    // Typing the word out: the menu is open while it is unfinished and stays open when the word is
    // complete, so it can move on to the next list at the space without disappearing in between.
    sh.send("git comm", 400);
    sh.send("it", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git commit] CUR=10 on=1"), "kept open, no blink");
    sh.send(" ", 400);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git commit ] CUR=11 on=1"), "moves on to the options: {s}");
    sh.send("\x15", 200);
    // Arriving at a finished word all at once (paste, history edit) has nothing to offer: it does not open.
    sh.send("git commit", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git commit] CUR=10 on=0"));
}

#[test]
fn live_min_waits_for_a_typed_character() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_LIVE_MIN", "1")], true);
    sh.send("git ", 400);
    assert!(fx.state(&mut sh).contains("on=0"), "a bare space does not open it");
    sh.send("c", 400);
    assert!(fx.state(&mut sh).contains("on=1"));
}
