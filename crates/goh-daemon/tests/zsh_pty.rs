//! End-to-end checks of adapters/zsh/goh.zsh in a real zsh on a pseudo-terminal, with the real
//! gohd/goh. The test plays the terminal: it answers OSC colour queries on request.
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
             dump() {{ print -r -- \"BUF[$BUFFER] CUR=$CURSOR on=$_goh_on sel=$_goh_sel post[${{POSTDISPLAY%%[[:cntrl:]]*}}]\" >> {d} }}\n\
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

/// The selected first row of the `git c` menu, as it reads on screen.
const SELECTED: &str = "❯ ↳ checkout";

/// What the terminal shows: `out` without its escape sequences (CSI, and OSC up to BEL or ST).
fn screen(out: &str) -> String {
    let (mut text, mut chars) = (String::new(), out.chars().peekable());
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

// ---- keyboard flow ---------------------------------------------------------------------------

#[test]
fn keyboard_flow() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[], true);
    let keymap_before = fx.keymap(&mut sh, 0);

    sh.send("git c", 150);
    let drawn = sh.send("\t", 500);
    for needle in [SELECTED, "↳ cherry-pick", "↳ commit", "Tab/→ insert"] {
        assert!(screen(&drawn).contains(needle), "menu should show {needle:?}: {drawn:?}");
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

    // At once, not after KEYTIMEOUT (0.4 s): nothing that starts with Esc is bound while the menu is open.
    let closed = sh.send("\x1b", 150);
    assert!(!closed.is_empty(), "Esc closes the menu within 150 ms");
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

    // Enter executes the typed line without inserting the highlighted suggestion.
    sh.send("git() { print -r -- RAN:$*; }\n", 200);
    sh.send("git c", 150);
    sh.send("\t", 500);
    let out = sh.send("\r", 500);
    assert!(out.contains("RAN:c"), "Enter executes the line as typed: {out:?}");
    assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"));
    assert_eq!(fx.keymap(&mut sh, 1), keymap_before);

}

#[test]
fn keys_starting_with_esc_still_work_while_the_menu_is_open() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[], true);
    let keymap_before = fx.keymap(&mut sh, 0);

    // Alt+B (backward-word) closes the menu and does what it does without it.
    sh.send("git c", 150);
    sh.send("\t", 500);
    sh.send("\x1bb", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git c] CUR=4 on=0"));
    assert_eq!(fx.keymap(&mut sh, 1), keymap_before);
    sh.send(CLEAR, 100);

    // Left closes and moves.
    sh.send("git c", 150);
    sh.send("\t", 500);
    sh.send("\x1b[D", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git c] CUR=4 on=0"));
    sh.send(CLEAR, 100);

    // A bracketed paste is pasted.
    sh.send("git c", 150);
    sh.send("\t", 500);
    sh.send("\x1b[200~ommit -m x\x1b[201~", 500);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git commit -m x] CUR=15"), "{s}");
    sh.send(CLEAR, 100);

    // Down and Right in one write: the menu moves, then takes the highlighted candidate.
    sh.send("git c", 150);
    sh.send("\t", 500);
    sh.send("\x1b[B\x1b[C", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git cherry-pick ] CUR=16 on=0"));
    assert_eq!(fx.keymap(&mut sh, 1), keymap_before);
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
        pty.send(&format!("PS1='READY> '; dump() {{ print -r -- \"BUF[$BUFFER] CUR=$CURSOR on=$_goh_on sel=$_goh_sel post[${{POSTDISPLAY%%[[:cntrl:]]*}}]\" >> {} }}; zle -N dump; bindkey '^Xd' dump\n", d.display()), 300);
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
        // The inline menu below the line is styled too; look at the span that wraps the ghost.
        let end = drawn.rfind("mheckout \x1b[39m").unwrap_or_else(|| panic!("{}: coloured ghost text in {drawn:?}", sc.name));
        let start = drawn[..end].rfind("\x1b[38;5;").unwrap_or_else(|| panic!("{}: colour before the ghost in {drawn:?}", sc.name));
        assert!(end - start < 16, "{}: colour spans exactly the ghost: {:?}", sc.name, &drawn[start..]);
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
fn overlay_start_and_confirmation_do_not_flash_an_inline_list() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()),
        ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "overlay"), ("FAKE_DELAY", "0.4")];
    let mut sh = fx.shell(&envs, true);
    sh.send(&format!("dumpdisplay() {{ print -r -- \"$_goh_overlay\" >> {}; }}; zle -N dumpdisplay; bindkey '^Xv' dumpdisplay\n", fx.path("display.txt").display()), 200);
    let initial = sh.send("git c", 100);
    assert!(!screen(&initial).contains(SELECTED), "do not draw a temporary inline list while starting the popup: {initial:?}");
    // Do not press a key or invoke an inspection widget until the display change has arrived.
    let idle = String::from_utf8_lossy(&sh.drain(Duration::from_millis(1000), Duration::from_secs(3))).into_owned();
    assert!(idle.is_empty(), "overlay confirmation should not redraw the terminal: {idle:?}");
    assert_eq!(fx.state_file(&mut sh, "\x18v", "display.txt"), "1", "the delayed overlay confirmation was received");
    assert_eq!(fx.state(&mut sh), "BUF[git c] CUR=5 on=1 sel=1 post[heckout ]", "handoff preserves the editable line and selection");
}

#[test]
fn an_unresponsive_popup_falls_back_without_blocking_typing() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()),
        ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "silent")];
    let mut sh = fx.shell(&envs, true);
    let start = Instant::now();
    let initial = sh.send("git c", 100);
    assert!(start.elapsed() < Duration::from_millis(700), "starting the popup must not wait for its deadline");
    assert!(!screen(&initial).contains(SELECTED), "{initial:?}");
    let idle = String::from_utf8_lossy(&sh.drain(Duration::from_millis(1300), Duration::from_secs(3))).into_owned();
    assert!(screen(&idle).contains(SELECTED), "a popup that never confirms needs an inline fallback while idle: {idle:?}");
    assert!(fx.state(&mut sh).contains("on=1"));
}

#[test]
fn a_popup_startup_timeout_does_not_reopen_a_dismissed_menu() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()),
        ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "silent")];
    let mut sh = fx.shell(&envs, true);
    sh.send("git c", 100);
    sh.send("\x03", 100);
    let idle = String::from_utf8_lossy(&sh.drain(Duration::from_millis(1300), Duration::from_secs(3))).into_owned();
    assert!(!screen(&idle).contains(SELECTED), "the old menu must not return at its deadline: {idle:?}");
    assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"));
}

#[test]
fn bare_command_frames_keep_the_whole_name_for_the_logo() {
    let fx = Fixture::new(require_env!(), false);
    let listener = goh_daemon::paths::bind(&fx.socket).unwrap().unwrap();
    let server = std::sync::Arc::new(common::isolated_server(goh_core::Engine::new(goh_core::bundled_specs()), fx.dir.path()));
    std::thread::spawn(move || server.serve(listener));
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "auto");
    sh.send("_GOH_LIVE=on\n", 200);
    for (typed, command) in [("docker", "docker"), ("docker ", "docker"), ("docker ps", "docker"), ("node", "node"), ("go", "go")] {
        let before = frames(&log).len();
        let (first, tail) = typed.split_once(' ').unwrap_or((typed, ""));
        sh.send(first, 400);
        if typed.contains(' ') { sh.send(&format!(" {tail}"), 300); }
        let f = wait_frames(&mut sh, &log, |f| f.iter().skip(before).any(|v| v["type"] == "show"));
        let show = f.iter().skip(before).rfind(|v| v["type"] == "show").expect("a fresh popup frame");
        assert_eq!(show["state"]["context"].as_str(), Some(command), "{typed:?}: the logo needs the complete command name");
        sh.send(CLEAR, 200);
    }

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
    assert_eq!(show["enter_accepts"], false, "untouched menus must not advertise Enter acceptance");
    assert!(show["theme"]["surface-base"].is_string(), "resolved theme tokens are sent");

    // The popup answered "overlay", so the adapter knows it is drawing.
    assert_eq!(fx.state_file(&mut sh, "\x18o", "ov.txt"), "ov=1 on=1 sel=1");

    // Down moves the selection in the popup too.
    sh.send("\x1b[B", 300);
    let f = wait_frames(&mut sh, &log, |f| f.iter().filter(|v| v["type"] == "show").count() >= 2);
    let show = f.iter().rfind(|v| v["type"] == "show").unwrap();
    assert_eq!(selected(show), 1);
    assert_eq!(show["enter_accepts"], true, "the footer follows explicit selection intent");

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
    assert!(screen(&drawn).contains(SELECTED), "inline menu is drawn: {drawn:?}");
    assert_eq!(fx.state_file(&mut sh, "\x18o", "ov.txt"), "ov=0 on=1 sel=1");
}

/// The inline menu's rows after a `git c` Tab menu that the fake popup answers inline with `reason`.
fn inline_answer_menu(fx: &Fixture, reason: &str) -> (Pty, Vec<String>) {
    let (log, popup) = (fx.path("popup.log"), fake_popup());
    let envs = [
        ("GOH_DISPLAY", "auto"),
        ("GOH_POPUP", popup.to_str().unwrap()),
        ("FAKE_LOG", log.to_str().unwrap()),
        ("FAKE_DISPLAY", "inline"),
        ("FAKE_REASON", reason),
        ("TERM_PROGRAM", "iTerm.app"),
    ];
    let mut sh = fx.shell(&envs, true);
    menu_dump(fx, &mut sh);
    sh.send("git c", 150);
    sh.send("\t", 900);
    wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    sh.drain(Duration::from_millis(300), Duration::from_secs(2));
    let (rows, _) = menu(fx, &mut sh);
    (sh, rows)
}

fn notice(rows: &[String]) -> bool {
    rows.join(" ").contains("Run: goh doctor --fix")
}

#[test]
fn the_accessibility_notice_stays_under_the_inline_menu_until_it_closes_and_comes_once() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, rows) = inline_answer_menu(&fx, "this terminal is not allowed to use the Accessibility API");
    assert!(notice(&rows) && rows.iter().all(|r| r.chars().count() <= 59), "wrapped under the menu: {rows:?}");
    sh.send("\x1b[B", 300);
    assert!(notice(&menu(&fx, &mut sh).0), "a key press does not take it away");
    sh.send("\x1b", 900);
    sh.send("\t", 900);
    let (rows, _) = menu(&fx, &mut sh);
    assert!(rows.len() > 2 && !notice(&rows), "once per shell: {rows:?}");
}

#[test]
fn other_accessibility_errors_do_not_claim_a_missing_permission() {
    let fx = Fixture::new(require_env!(), true);
    let (_sh, rows) = inline_answer_menu(&fx, "no focused application (Accessibility error -25204)");
    assert!(rows.len() > 2 && !notice(&rows), "{rows:?}");
}

#[test]
fn display_inline_never_starts_the_popup() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "inline");
    sh.send("git c", 150);
    let drawn = sh.send("\t", 900);
    assert!(screen(&drawn).contains(SELECTED));
    sh.send("\x1b[B", 300);
    assert!(!log.exists(), "no popup process should have been started");
}

#[test]
fn a_dead_popup_fails_open_and_never_blocks_the_shell() {
    let fx = Fixture::new(require_env!(), true);
    // /usr/bin/false exits at once: frames go nowhere, decisions never come.
    let (mut sh, _log) = overlay_shell(&fx, "/usr/bin/false", "overlay", "auto");
    sh.send("git c", 150);
    let drawn = sh.send("\t", 1300);
    assert!(screen(&drawn).contains(SELECTED), "inline menu still appears: {drawn:?}");
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
        "goh    /",
        "gohd       socket ",
        "specs      ",
        "next daemon start; running daemon may differ",
        "restart gohd after changes",
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
fn bundled_specs_keep_suggesting_after_subcommands_and_positional_arguments() {
    let fx = Fixture::new(require_env!(), false);
    fs::write(fx.path("app.js"), "").unwrap();
    let mut sh = fx.shell(LIVE, true);
    for (typed, expected, ghost) in [
        ("docker exec --int", "docker exec --interactive ", "eractive "),
        ("go test ./... -ru", "go test ./... -run ", "n "),
        ("node ap", "node app.js ", "p.js "),
    ] {
        sh.send(typed, 500);
        let state = fx.state(&mut sh);
        assert!(state.contains("on=1") && state.ends_with(&format!("post[{ghost}]")), "{typed}: {state}");
        sh.send("\t", 300);
        assert!(fx.state(&mut sh).starts_with(&format!("BUF[{expected}]")));
        sh.send(CLEAR, 100);
    }
    kill_socket_holder(&fx.socket);
}

#[test]
fn accepting_a_directory_reopens_the_menu_for_its_contents_but_a_command_does_not() {
    let fx = Fixture::new(require_env!(), false);
    fs::create_dir_all(fx.path("project/src")).unwrap();
    fs::write(fx.path("project/README.md"), "").unwrap();
    let mut sh = fx.shell(LIVE, true);
    sh.send("cd pro", 500);
    assert!(fx.state(&mut sh).contains("on=1") && fx.state(&mut sh).ends_with("post[ject/]"));
    sh.send("\t", 800);
    let state = fx.state(&mut sh);
    assert!(state.starts_with("BUF[cd project/] CUR=11 on=1"), "the directory's contents open without deleting anything: {state}");
    assert!(state.ends_with("post[src/]") || state.ends_with("post[README.md ]"), "{state}");
    sh.send("\t", 800);
    assert!(fx.state(&mut sh).starts_with("BUF[cd project/"), "the next pick goes on from there");
    sh.send(CLEAR, 100);

    // A command or subcommand is accepted with its space and does not reopen by itself.
    sh.send("git c", 500);
    sh.send("\t", 800);
    let state = fx.state(&mut sh);
    assert!(state.starts_with("BUF[git checkout ] CUR=13 on=0"), "{state}");
    sh.send(CLEAR, 100);

    // The same with `cd` aliased to zoxide's `z`, a function no spec describes: it still completes as `cd`.
    sh.send("z() { builtin cd \"$@\" }; alias cd=z\n", 800);
    sh.send("cd pro", 500);
    assert!(fx.state(&mut sh).ends_with("post[ject/]"), "the alias opens the menu as `cd` does");
    sh.send("\t", 800);
    let state = fx.state(&mut sh);
    assert!(state.starts_with("BUF[cd project/] CUR=11 on=1"), "{state}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn a_short_visible_menu_keeps_more_results_and_scrolls_past_its_last_row() {
    let fx = Fixture::new(require_env!(), false);
    let subcommands: Vec<_> = (0..12).map(|i| serde_json::json!({"name": format!("item-{i:02}")})).collect();
    fs::write(fx.path("specs/git.json"), serde_json::json!({"schemaVersion":1,"name":"git","subcommands":subcommands}).to_string()).unwrap();
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_LIMIT", "3")], true);
    sh.send(&format!("dumpwindow() {{ print -r -- \"sel=$_goh_sel win=$_goh_win total=$#_goh_label\" >> {}; }}; zle -N dumpwindow; bindkey '^Xw' dumpwindow\n", fx.path("window.txt").display()), 200);
    let initial = sh.send("git ", 400);
    assert!(!initial.contains("item-03"), "the inline list must only draw three rows: {initial:?}");
    sh.send(&"\x1b[B".repeat(7), 400);
    assert_eq!(fx.state_file(&mut sh, "\x18w", "window.txt"), "sel=8 win=6 total=12");
    sh.send("\t", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git item-07 ] CUR=12 on=0"));
    kill_socket_holder(&fx.socket);
}

#[test]
fn popup_scroll_updates_the_shell_selection_and_rejects_stale_or_invalid_events() {
    let fx = Fixture::new(require_env!(), true);
    let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "auto");
    sh.send(&format!("dumppipe() {{ print -r -- \"$_goh_pop_dir\" >> {}; }}; zle -N dumppipe; bindkey '^Xp' dumppipe\n", fx.path("pipe.txt").display()), 200);
    sh.send("git c\t", 600);
    let f = wait_frames(&mut sh, &log, |f| f.iter().any(|v| v["type"] == "show"));
    let id = f.iter().rfind(|v| v["type"] == "show").unwrap()["state"]["id"].as_str().unwrap().to_string();
    let dir = fx.state_file(&mut sh, "\x18p", "pipe.txt");
    let mut pipe = fs::OpenOptions::new().write(true).open(Path::new(&dir).join("out")).unwrap();
    writeln!(pipe, "{}", serde_json::json!({"type":"navigate","id":id,"delta":2})).unwrap();
    sh.drain(Duration::from_millis(200), Duration::from_millis(500));
    assert!(fx.state(&mut sh).contains("on=1 sel=3 post[ommit ]"));
    for (event_id, delta) in [("old-menu", -2), (id.as_str(), 1000000)] {
        writeln!(pipe, "{}", serde_json::json!({"type":"navigate","id":event_id,"delta":delta})).unwrap();
        sh.drain(Duration::from_millis(100), Duration::from_millis(300));
        assert!(fx.state(&mut sh).contains("on=1 sel=3"));
    }
    writeln!(pipe, "{}", serde_json::json!({"type":"navigate","id":id,"delta":100})).unwrap();
    sh.drain(Duration::from_millis(100), Duration::from_millis(300));
    assert!(fx.state(&mut sh).contains("sel=3"), "scrolling at the bottom must not wrap");
    sh.send("\x1b[C", 200);
    assert!(fx.state(&mut sh).starts_with("BUF[git commit ] CUR=11 on=0"), "accept must insert the visibly selected row");
    sh.send("\x15git c\t", 400);
    let frames = wait_frames(&mut sh, &log, |frames| frames.iter().any(|v| v["type"] == "show" && v["state"]["id"].as_str().is_some_and(|s| s != id)));
    let new_id = frames.iter().rfind(|v| v["type"] == "show").unwrap()["state"]["id"].as_str().unwrap();
    assert_ne!(new_id, id, "reopening the same query must invalidate queued events");
    for (event_id, delta) in [(id.as_str(), 2), (new_id, -100)] {
        writeln!(pipe, "{}", serde_json::json!({"type":"navigate","id":event_id,"delta":delta})).unwrap();
        sh.drain(Duration::from_millis(100), Duration::from_millis(300));
        assert!(fx.state(&mut sh).contains("on=1 sel=1"), "stale events and scrolling above the top must not move selection");
    }
}

#[test]
fn magic_space_refreshes_an_open_command_menu() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send("bindkey ' ' magic-space\n", 200);
    sh.send("git", 300);
    assert!(fx.state(&mut sh).contains("on=1"));
    sh.send(" ", 300);
    assert_eq!(fx.state(&mut sh), "BUF[git ] CUR=4 on=1 sel=1 post[checkout ]");
    sh.send("c", 300);
    assert_eq!(fx.state(&mut sh), "BUF[git c] CUR=5 on=1 sel=1 post[heckout ]");
}

#[test]
fn typing_an_exact_command_opens_its_subcommands_without_a_space() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send("git", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git] CUR=3 on=1 sel=1 post[ checkout ]");
    sh.send("\x1b[B\x1b[C", 300);
    assert_eq!(fx.state(&mut sh), "BUF[git cherry-pick ] CUR=16 on=0 sel=2 post[]");
    sh.send(CLEAR, 100);
    sh.send("git", 400);
    sh.send(" ", 300);
    sh.send("\t", 300);
    assert_eq!(fx.state(&mut sh), "BUF[git checkout ] CUR=13 on=0 sel=1 post[]", "typing the separator must not double it on accept");
}

#[test]
fn the_inline_menu_is_drawn_below_the_line_with_the_selected_row_highlighted() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send(
        &format!("dumpmenu() {{ print -r -- \"POST[${{POSTDISPLAY//[[:cntrl:]]/|}}] H:[${{(j:;:)region_highlight}}]\" >> {}; }}; zle -N dumpmenu; bindkey '^Xm' dumpmenu\n", fx.path("menu.txt").display()),
        300,
    );
    sh.send("git c", 400);
    let s = fx.state_file(&mut sh, "\x18m", "menu.txt");
    assert!(s.contains("heckout |❯ ↳ checkout|  ↳ cherry-pick|  ↳ commit|  Switch branches|  1/3 · Tab/→ insert"), "ghost, rows, detail and footer below the line: {s}");
    assert!(s.split(';').any(|h| h.starts_with("P14 15 ") && h.contains("bold")), "the selected marker is bold, not only coloured: {s}");
    assert!(!s.contains("standout"), "no reverse-video row: {s}");
}

/// Bind `^Xm` to dump the inline menu (rows joined by `|`) and the highlights to `menu.txt`.
fn menu_dump(fx: &Fixture, sh: &mut Pty) {
    sh.send(
        &format!("dumpmenu() {{ print -r -- \"POST[${{POSTDISPLAY//[[:cntrl:]]/|}}] H:[${{(j:;:)region_highlight}}]\" >> {}; }}; zle -N dumpmenu; bindkey '^Xm' dumpmenu\n", fx.path("menu.txt").display()),
        300,
    );
}

/// The menu's rows (ghost line first) and its highlights.
fn menu(fx: &Fixture, sh: &mut Pty) -> (Vec<String>, String) {
    let s = fx.state_file(sh, "\x18m", "menu.txt");
    let (post, h) = s.split_once("] H:[").unwrap();
    (post.trim_start_matches("POST[").split('|').map(str::to_string).collect(), h.trim_end_matches(']').to_string())
}

#[test]
fn the_best_line_from_the_zsh_history_is_ghost_text_that_right_arrow_takes() {
    let fx = Fixture::new(require_env!(), false);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::write(fx.path(".zsh_history"), format!(": {}:0;git commit -m wip\n: {}:0;git checkout -b done-once\n", now - 300, now - 200)).unwrap();
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_RECORD", "on")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 1500);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[0], "ommit -m wip", "the ghost is the history line, not the selected `checkout`: {rows:?}");
    assert!(rows.iter().any(|r| r.contains("→ line · Tab insert")), "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains("done-once")), "a branch created once is not offered again: {rows:?}");
    sh.send("\x1b[C", 800);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git commit -m wip]"), "→ takes the whole line: {s}");
    sh.send(CLEAR, 300);
    sh.send("git c", 1500);
    sh.send("\t", 800);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git checkout"), "Tab still takes the selected suggestion: {s}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn a_secret_in_the_zsh_history_is_shown_masked_and_taken_whole() {
    let fx = Fixture::new(require_env!(), false);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::write(fx.path(".zsh_history"), format!(": {}:0;git push https://me:hunter2@example.com/r.git main\n", now - 300)).unwrap();
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_RECORD", "on")], true);
    menu_dump(&fx, &mut sh);
    let drawn = sh.send("git pu", 1500);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[0], "sh https://me:****@example.com/r.git main", "the ghost is the masked line: {rows:?}");
    assert!(!drawn.contains("hunter2") && !rows.iter().any(|r| r.contains("hunter2")), "the secret is never drawn: {rows:?}");
    sh.send("\x1b[C", 800);
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git push https://me:hunter2@example.com/r.git main]"), "→ takes the line itself: {s}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn rows_show_kind_glyphs_learned_badges_and_every_rows_warning() {
    let fx = Fixture::new(require_env!(), false);
    let push = serde_json::json!({"schemaVersion":1,"name":"git","subcommands":[{"name":"push","options":[
        {"name":"--force","description":"Force"},
        {"name":"--repo","description":"Repository","takesValue":true,"args":[{"name":"repo"}]},
        {"name":"--delete","description":"Delete refs"},
        {"name":"--tags","description":"Push all tags"}]}]});
    fs::write(fx.path("specs/git.json"), push.to_string()).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::create_dir_all(fx.path(".local/share/goh")).unwrap();
    fs::write(fx.path(".local/share/goh/usage"), format!("{now}\t/elsewhere\tgit push\t--tags\n")).unwrap();
    let mut sh = fx.shell(LIVE, true);
    menu_dump(&fx, &mut sh);
    sh.send("git push --", 600);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[1], "❯ ① --tags", "a learned pick ranks first and takes a badge: {rows:?}");
    let row = |label: &str| rows.iter().find(|r| r.contains(label)).cloned().unwrap_or_default();
    assert!(row("--repo").starts_with("  ◇ --repo"), "an option that takes a value: {rows:?}");
    assert!(row("--force").starts_with("  ⚑ --force") && row("--force").ends_with("⚠ overwrites the remote branch"), "an unselected row shows its warning: {rows:?}");
    assert!(row("--delete").ends_with("⚠ deletes the remote branch or tag"), "{rows:?}");
    assert_eq!(rows[rows.len() - 2], "  Push all tags", "the detail line describes the selected row: {rows:?}");

    while !menu(&fx, &mut sh).0.iter().any(|r| r.starts_with("❯ ⚑ --force")) {
        sh.send("\x1b[B", 200);
    }
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[rows.len() - 2], "  Force", "the warning is on the row, not repeated in the detail line: {rows:?}");
    kill_socket_holder(&fx.socket);
}

fn fake_autosuggest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-autosuggest.zsh")
}

/// A live shell with the zsh-autosuggestions stand-in loaded before the adapter, as plugin managers do.
fn autosuggest_shell(fx: &Fixture) -> Pty {
    let mut sh = fx.shell(LIVE, false);
    sh.send(&format!("source {}\nsource {}\n", fake_autosuggest().display(), adapter().display()), 900);
    menu_dump(fx, &mut sh);
    sh
}

#[test]
fn a_late_autosuggestion_does_not_wipe_the_inline_menu() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = autosuggest_shell(&fx);
    sh.send("git c", 900); // the plugin's answer for `git c` lands 200 ms after the menu opened
    let (rows, _) = menu(&fx, &mut sh);
    assert!(rows.len() > 3 && rows[1] == SELECTED && !rows[0].contains("from-history"), "{rows:?}");
    sh.send("\x1b", 900);
    sh.send("x", 900);
    assert!(fx.state(&mut sh).ends_with("post[-from-history]"), "with the menu closed the plugin suggests again");
}

#[test]
fn an_autosuggest_accept_key_closes_the_menu_instead_of_taking_its_text() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = autosuggest_shell(&fx);
    sh.send("git c", 900);
    sh.send("\x05", 400); // ^E: autosuggest-accept
    let s = fx.state(&mut sh);
    assert!(s.starts_with("BUF[git c] CUR=5 on=0"), "the line keeps what was typed: {s}");
    sh.send("\x18m", 300);
    assert_eq!(fx.keymap(&mut sh, 1).lines().find(|l| l.starts_with("bindkey \"^E\"")), Some("bindkey \"^E\" autosuggest-accept"), "its own key is back");
}

#[test]
fn pausing_and_resuming_the_plugin_prints_no_error_into_the_line() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = autosuggest_shell(&fx);
    let mut out = sh.send("git c", 900); // the menu opens: the plugin is paused
    out += &sh.send("\x1b", 900); // and closes: it is resumed
    assert!(!out.contains("parameter not set"), "the plugin's widgets run without goh's no_unset: {:?}", screen(&out));
}

#[test]
fn a_plugin_you_turned_off_stays_off_after_the_menu() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = autosuggest_shell(&fx);
    sh.send("typeset -g _ZSH_AUTOSUGGEST_DISABLED\n", 300);
    sh.send("git c", 900);
    sh.send("\x1b", 900);
    sh.send("x", 900);
    assert!(fx.state(&mut sh).ends_with("post[]"), "goh only resumes what it paused");
}

#[test]
fn goh_reload_loads_the_adapter_again_with_the_config_read_again() {
    let fx = Fixture::new(require_env!(), false);
    fs::write(fx.path("no-config.json"), r#"{"display":{"maxSuggestions":3}}"#).unwrap();
    let mut sh = fx.shell(&[], true);
    assert!(screen(&sh.send("print -r -- LIMIT=$_GOH_LIMIT\n", 300)).contains("LIMIT=3"));
    fs::write(fx.path("no-config.json"), r#"{"display":{"maxSuggestions":5}}"#).unwrap();
    let out = screen(&sh.send("goh reload\n", 2500));
    assert!(out.contains("reloaded: gohd now serves") && out.contains("adapter is loaded again too"), "{out:?}");
    assert!(screen(&sh.send("print -r -- LIMIT=$_GOH_LIMIT TAB=$_GOH_ORIG_TAB\n", 300)).contains("LIMIT=5 TAB=expand-or-complete"), "Tab still has its own widget to fall back to");
    sh.send("git c", 150);
    assert!(screen(&sh.send("\t", 600)).contains(SELECTED), "the reloaded adapter completes");
    kill_socket_holder(&fx.socket);
}

#[test]
fn no_badges_while_learned_choices_are_off() {
    let fx = Fixture::new(require_env!(), false);
    fs::write(fx.path("no-config.json"), r#"{"privacy":{"learnChoices":false}}"#).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::create_dir_all(fx.path(".local/share/goh")).unwrap();
    fs::write(fx.path(".local/share/goh/usage"), format!("{now}\t/x\tgit\tcommit\n")).unwrap();
    let mut sh = fx.shell(LIVE, true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 600);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(&rows[1..4], ["❯ ↳ checkout", "  ↳ cherry-pick", "  ↳ commit"], "{rows:?}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn no_color_keeps_every_cue_but_colour() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("NO_COLOR", "1")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, h) = menu(&fx, &mut sh);
    assert_eq!(rows[1], "❯ ↳ checkout");
    let menu_styles: Vec<&str> = h.split(';').filter(|e| !e.starts_with("P5 ")).collect(); // P5: the ghost text
    assert!(menu_styles.iter().all(|e| !e.contains("fg=")), "no colour in the menu: {h}");
    assert!(menu_styles.iter().any(|e| e.starts_with("P14 15 bold")), "the marker is still bold: {h}");
    assert!(menu_styles.iter().any(|e| e.contains("bold,underline")), "matches still stand out: {h}");
}

#[test]
fn a_narrow_terminal_shortens_the_footer_and_clips_every_row() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    menu_dump(&fx, &mut sh);
    sh.send("COLUMNS=24\n", 300);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows.last().unwrap(), "  1/3 · ↑↓ Tab/→ Esc", "{rows:?}");
    assert!(rows[1..].iter().all(|r| r.chars().count() <= 23), "{rows:?}");
}

#[test]
fn a_long_list_shows_its_position_and_keeps_its_label_column() {
    let fx = Fixture::new(require_env!(), false);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_LIMIT", "5")], true);
    menu_dump(&fx, &mut sh);
    sh.send("tmux ", 600);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows.len(), 1 + 5 + 2, "five rows, detail and footer: {rows:?}");
    assert!(rows.last().unwrap().starts_with("  1/50+ · "), "164 subcommands, 50 asked for: {rows:?}");
    sh.send("\x1b[A", 300); // wraps to the last one
    let (rows, _) = menu(&fx, &mut sh);
    assert!(rows.last().unwrap().starts_with("  50/50+ · "), "{rows:?}");
    sh.send(CLEAR, 100);
    sh.send("tsc --", 600);
    let (rows, _) = menu(&fx, &mut sh);
    assert!(rows[1..6].iter().all(|r| r.contains("◇ --") || r.contains("⚑ --")), "options and flags: {rows:?}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn ghost_color_is_removed_after_refresh_accept_and_dismissal() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    sh.send(
        &format!(
            "zmodload zsh/nearcolor; _goh_ghost_style='fg=#8fa99a'; \
             dumpcolors() {{ print -r -- \"BUF[$BUFFER] POST[${{POSTDISPLAY%%[[:cntrl:]]*}}] H:[${{(j:;:)region_highlight}}]\" >> {}; }}; \
             foreign_color() {{ region_highlight+=('0 1 bold'); }}; \
             zle -N dumpcolors; zle -N foreign_color; bindkey '^Xc' dumpcolors; bindkey '^Xh' foreign_color\n",
            fx.path("colors.txt").display(),
        ),
        300,
    );
    for key in ["\t", "\x1b[C"] {
        sh.send("git c", 300);
        sh.send("\x18h", 100); // unrelated highlighting must survive our cleanup
        sh.send("h", 300); // redraw the ghost at a new span
        let s = fx.state_file(&mut sh, "\x18c", "colors.txt");
        assert_eq!(s.matches(";P6 ").count(), 1, "refresh must replace, not accumulate, ghost colors: {s}");
        sh.send(key, 300);
        let s = fx.state_file(&mut sh, "\x18c", "colors.txt");
        assert_eq!(s, "BUF[git checkout ] POST[] H:[0 1 bold]", "{key:?}: accepted text has no ghost color");
        // Remove only the test's highlight before the next case.
        sh.send(CLEAR, 100);
        sh.send("region_highlight=()\n", 200);
    }
    sh.send("git c", 300);
    sh.send("\x1b", 900);
    let s = fx.state_file(&mut sh, "\x18c", "colors.txt");
    assert_eq!(s, "BUF[git c] POST[] H:[]", "dismissal removes ghost coloring");
    sh.send(CLEAR, 100);
    // Emulate a shell without memo support: suggestions still work without adding unsafe colors.
    sh.send("is-at-least() { return 1; }\n", 200);
    sh.send("git c", 300);
    let s = fx.state_file(&mut sh, "\x18c", "colors.txt");
    assert_eq!(s, "BUF[git c] POST[heckout ] H:[]");
    sh.send("\t", 300);
    let s = fx.state_file(&mut sh, "\x18c", "colors.txt");
    assert_eq!(s, "BUF[git checkout ] POST[] H:[]");
}

#[test]
fn ctrl_c_hides_the_overlay_and_restores_keys_at_the_next_prompt() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()), ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "overlay")];
    let mut sh = fx.shell(&envs, true);
    let keys = fx.keymap(&mut sh, 0);
    for _ in 0..2 {
        sh.send("git c", 400);
        assert!(fx.state(&mut sh).contains("on=1"));
        sh.send("\x03", 200);
        let s = fx.state(&mut sh);
        assert!(s.starts_with("BUF[] CUR=0 on=0") && s.ends_with("post[]"), "{s}");
        let f = frames(&log);
        assert_eq!(f.last().unwrap()["type"], "hide", "Ctrl+C must send hide: {f:?}");
        assert_eq!(fx.keymap(&mut sh, 1), keys, "interrupt restores temporary bindings");
    }
}

#[test]
fn ctrl_c_hides_the_overlay_even_when_a_plugin_replaces_zle_line_init() {
    let fx = Fixture::new(require_env!(), true);
    let log = fx.path("popup.log");
    let popup = fake_popup();
    let envs = [("GOH_LIVE", "on"), ("GOH_DISPLAY", "auto"), ("GOH_POPUP", popup.to_str().unwrap()), ("FAKE_LOG", log.to_str().unwrap()), ("FAKE_DISPLAY", "overlay")];
    let mut sh = fx.shell(&envs, true);
    // Loaded after the adapter, like a plugin sourced later in .zshrc: it takes the hook widget over.
    sh.send("zle-line-init() { :; }; zle -N zle-line-init zle-line-init\n", 300);
    let keys = fx.keymap(&mut sh, 0);
    for _ in 0..2 {
        sh.send("git c", 400);
        assert!(fx.state(&mut sh).contains("on=1"));
        sh.send("\x03", 200);
        let s = fx.state(&mut sh);
        assert!(s.starts_with("BUF[] CUR=0 on=0") && s.ends_with("post[]"), "{s}");
        let f = frames(&log);
        assert_eq!(f.last().unwrap()["type"], "hide", "Ctrl+C must send hide: {f:?}");
        assert_eq!(fx.keymap(&mut sh, 1), keys, "interrupt restores temporary bindings");
    }
}

#[test]
fn a_newer_release_is_announced_once_and_only_when_updates_check_is_on() {
    let fx = Fixture::new(require_env!(), false);
    let state = fx.path("update-state");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::write(&state, format!(r#"{{"checked":{now},"latest":"99.0.0"}}"#)).unwrap();
    for (on, expect) in [("on", true), ("off", false)] {
        let envs = [("GOH_UPDATE_CHECK", on), ("GOH_UPDATE_STATE", state.to_str().unwrap())];
        let mut sh = fx.shell(&envs, false);
        let first = sh.send(&format!("source {}\n", adapter().display()), 900);
        assert_eq!(first.contains("goh 99.0.0 is available"), expect, "updates.check {on}: {first:?}");
        let second = sh.send("\n", 400);
        assert!(!second.contains("is available"), "told again at the next prompt: {second:?}");
        let _ = fs::write(&state, format!(r#"{{"checked":{now},"latest":"99.0.0"}}"#)); // forget that it was told
    }
}

#[test]
fn live_tab_accepts_an_open_menu_but_does_not_open_a_dismissed_one() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, false);
    sh.send("native_tab() { LBUFFER+='NATIVE'; }; zle -N native_tab; bindkey '^I' native_tab\n", 200);
    sh.send(&format!("source {}\n", adapter().display()), 700);
    sh.send("git c", 400);
    assert!(fx.state(&mut sh).contains("on=1"));
    sh.send("\t", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git checkout ] CUR=13 on=0"));
    sh.send(CLEAR, 100);
    sh.send("git c", 400);
    sh.send("\x1b", 900);
    sh.send("\t", 300);
    assert!(fx.state(&mut sh).starts_with("BUF[git cNATIVE] CUR=11 on=0"), "closed menu delegates Tab to the original widget");
}

#[test]
fn enter_after_a_menu_redraws_the_complete_multiline_prompt() {
    let locale = require_env!();
    for display in ["inline", "overlay"] {
        let fx = Fixture::new(locale.clone(), true);
        let (mut sh, log) = overlay_shell(&fx, fake_popup().to_str().unwrap(), display, display);
        sh.send("PS1=$'prompt-top\\n%F{green}❯%f '; git() { print -r -- RAN:$*; }; _GOH_LIVE=on\n", 300);
        for enter in ["\r", "\n"] {
            sh.send("git c", 400);
            assert!(fx.state(&mut sh).contains("on=1"), "{display}: menu must be open");
            let output = sh.send(enter, 400);
            let prompt = output.rsplit_once("RAN:c").expect("Enter must execute only the typed line").1;
            assert!(prompt.contains("prompt-top") && prompt.contains('❯'), "{display} {enter:?}: next prompt lost text: {prompt:?}");
            assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"));
            if display == "overlay" {
                let f = wait_frames(&mut sh, &log, |f| f.last().is_some_and(|v| v["type"] == "hide"));
                assert_eq!(f.last().unwrap()["type"], "hide");
            }
        }
        // The following command has no menu, as in the user's `pwd` screenshot.
        let output = sh.send("pwd\r", 400);
        assert!(output.contains("prompt-top") && output.contains('❯'), "{display}: subsequent prompts must stay complete: {output:?}");
    }
}

#[test]
fn accepting_a_directory_redraws_the_line_in_place_under_a_multiline_prompt() {
    let fx = Fixture::new(require_env!(), false);
    fs::create_dir_all(fx.path("project/src")).unwrap();
    let (mut sh, _) = overlay_shell(&fx, fake_popup().to_str().unwrap(), "overlay", "overlay");
    sh.send("PS1=$'prompt-top\\n❯ '; _GOH_LIVE=on\n", 300);
    sh.send("cd pro", 500);
    let output = sh.send("\t", 800);
    assert!(fx.state(&mut sh).starts_with("BUF[cd project/] CUR=11 on=1"), "the menu reopens for the directory");
    // The line is one row and the overlay draws nothing in it: moving the cursor up lands on the prompt's first line.
    assert!(!output.contains("\x1b[A"), "the line was redrawn above itself: {output:?}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn accepted_text_is_not_left_in_another_plugins_suggestion_highlight() {
    let fx = Fixture::new(require_env!(), false);
    fs::create_dir_all(fx.path("project/src")).unwrap();
    let mut sh = fx.shell(LIVE, true);
    // As zsh-autosuggestions does: self-insert replaced by a wrapper that shows a suggestion after the line and
    // highlights it from the end of BUFFER.
    let rh = fx.path("rh.txt");
    sh.send(
        &format!(
            "_sugg() {{ zle .self-insert; POSTDISPLAY=hint; region_highlight+=(\"$#BUFFER $(( $#BUFFER + 4 )) fg=8\") }}; zle -N self-insert _sugg\n\
             rh() {{ print -r -- \"${{(j:;:)region_highlight}}\" >> {} }}; zle -N rh; bindkey '^Xh' rh\n",
            rh.display()
        ),
        300,
    );
    sh.send("cd pro", 500);
    sh.send("\t", 800);
    assert!(fx.state(&mut sh).starts_with("BUF[cd project/] CUR=11"));
    let highlights = fx.state_file(&mut sh, "\x18h", "rh.txt");
    assert!(!highlights.contains("fg=8"), "the suggestion's highlight would dim `project/`: {highlights}");
    kill_socket_holder(&fx.socket);
}

#[test]
fn enter_takes_after_navigation_or_a_lone_candidate_and_runs_the_line() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    let ran = fx.path("ran.txt");
    sh.send(&format!("git() {{ print -r -- \"$*\" >> {}; }}\n", ran.display()), 300);
    let keys = fx.keymap(&mut sh, 0);
    for key in ["\x1b[C", "\x1bOC"] {
        sh.send("git c", 400);
        sh.send("\x1b[B", 200);
        sh.send(key, 300);
        let s = fx.state(&mut sh);
        assert!(s.starts_with("BUF[git cherry-pick ] CUR=16 on=0"), "{key:?}: {s}");
        assert!(!ran.exists(), "accepting {key:?} must not execute git");
        assert_eq!(fx.keymap(&mut sh, 1), keys);
        sh.send(CLEAR, 100);
    }
    for key in ["\x1b[D", "\x1bOD"] {
        sh.send("git c", 400);
        sh.send(key, 300);
        let s = fx.state(&mut sh);
        assert!(s.starts_with("BUF[git c] CUR=4 on=0") && s.ends_with("post[]"), "{key:?}: {s}");
        assert_eq!(fx.keymap(&mut sh, 1), keys);
        sh.send(CLEAR, 100);
    }
    for key in ["\r", "\n"] {
        sh.send("git c", 400);
        sh.send(key, 400);
        assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"), "untouched Enter runs the typed line");
        sh.send("git c", 400);
        sh.send("\x1b[B", 200);
        sh.send(key, 400);
        assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"), "navigated Enter takes the candidate and runs the line");
        assert_eq!(fx.keymap(&mut sh, 1), keys);
    }
    // Editing starts a new query and removes Enter's navigation intent.
    sh.send("git c", 400);
    sh.send("\x1b[B", 200);
    sh.send("h", 300);
    sh.send("\r", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"));
    // A lone candidate needs no navigation: Enter takes it and runs the line.
    sh.send("git chec", 400);
    assert_eq!(fx.state(&mut sh), "BUF[git chec] CUR=8 on=1 sel=1 post[kout ]");
    sh.send("\r", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"), "Enter takes the only candidate and runs it");
    assert_eq!(fs::read_to_string(&ran).unwrap(), "c\ncherry-pick\nc\ncherry-pick\nch\ncheckout\n");
    // A line that cannot be complete yet is only inserted: an option that wants its value.
    sh.send("git -", 500);
    let s = fx.state(&mut sh);
    sh.send("\r", 600);
    let after = fx.state(&mut sh);
    assert!(after.starts_with("BUF[git -C "), "Enter inserts an option that takes a value without running: {s} -> {after}");
    sh.send(CLEAR, 100);
    // display.enterRuns off: Enter only inserts, and the next Enter runs.
    sh.send("GOH_ENTER_RUNS=off\n", 300);
    sh.send("git chec", 400);
    sh.send("\r", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[git checkout ] CUR=13 on=0"), "with enterRuns off Enter only inserts");
    sh.send("\r", 400);
    assert!(fx.state(&mut sh).starts_with("BUF[] CUR=0 on=0"));
}

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

#[test]
fn an_alias_defined_in_the_shell_completes_as_its_command() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(LIVE, true);
    // Defined after the adapter loaded, as a `.zshrc` that sources goh first would: the next prompt reports it.
    sh.send("alias g=git\n", 800);
    sh.send("g c", 600);
    assert_eq!(fx.state(&mut sh), "BUF[g c] CUR=3 on=1 sel=1 post[heckout ]");
}

const BOX: &[(&str, &str)] = &[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "box")];

#[test]
fn the_box_frames_the_rows_under_the_word_being_completed() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, h) = menu(&fx, &mut sh);
    // rows[0] is the ghost text on the line itself; the box follows.
    let top = &rows[1];
    let indent = top.chars().take_while(|c| *c == ' ').count();
    let inner = top.trim_start().chars().count() - 2;
    assert!(top.trim_start().starts_with('╭') && top.ends_with('╮'), "{rows:?}");
    // `READY> ` is 7 columns and `git ` 4 more: the word starts at column 11, unless the box must move left to fit 60.
    assert_eq!(indent, 11.min(59 - inner - 2), "{rows:?}");
    let body: Vec<&String> = rows[2..].iter().take_while(|r| !r.trim_start().starts_with('╰')).collect();
    assert!(body.iter().all(|r| r.chars().count() == indent + inner + 2 && r[..indent].trim().is_empty()), "every row has the box's width: {rows:?}");
    assert_eq!(body[0].trim_start(), format!("│{:inner$}│", "❯ ↳ checkout"));
    assert!(rows.iter().any(|r| r.trim_start().starts_with('╰')), "closed at the bottom: {rows:?}");
    assert!(h.contains("bold"), "the selected row stays bold: {h}");
    // Highlights are `P<start> <end> <style>` counted from the start of the line; they must land on the box.
    let row = row_offset(&rows, 2, 5);
    let marker = row + indent + 1;
    assert!(styles_at(&h, marker, marker + 1).any(|st| st.contains("bold")), "the bold `❯` sits on the selected row's marker (P{marker}): {h}");
    assert!(styles_at(&h, marker, marker + inner).any(|st| st.starts_with("bg=")), "the selected row's fill spans the inside of the box, P{marker} to P{}: {h}", marker + inner);
    let top_at = row_offset(&rows, 1, 5) + indent;
    assert!(styles_at(&h, top_at, top_at + inner + 2).any(|st| st.starts_with("fg=")), "the top border starts at the corner (P{top_at}): {h}");
    // zsh 5.9 drops a fill's background under a later style that sets a colour, so each style on a fill repeats it:
    // otherwise the marker, glyph and label of the selected row show the terminal's background through the fill.
    let fill = styles_at(&h, marker, marker + inner).find(|st| st.starts_with("bg=")).unwrap();
    let fill = fill.split([',', ' ']).next().unwrap().to_string();
    let on_selected: Vec<&str> = h
        .split(';')
        .filter(|e| {
            let mut n = e.trim_start_matches('P').split(' ').map(|x| x.parse::<usize>().unwrap_or(usize::MAX));
            let (s, end) = (n.next().unwrap(), n.next().unwrap_or(usize::MAX));
            s >= marker && end <= marker + inner && e.contains("memo=goh-ghost")
        })
        .collect();
    assert!(on_selected.len() > 1, "the selected row has styles over its fill: {h}");
    assert!(on_selected.iter().all(|e| e.contains(&fill)), "every style on the selected row keeps {fill}: {on_selected:?}");
    assert!(styles_at(&h, top_at, top_at + inner + 2).filter(|st| st.starts_with("fg=")).all(|st| st.contains(",bg=")), "the border keeps the box's fill: {h}");
}

#[test]
fn the_box_measures_a_prompt_subst_prompt_after_expanding_its_parameters() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    sh.send("COLUMNS=100; setopt prompt_subst; GOH_PROMPT_LABEL=0123456789; PS1='${GOH_PROMPT_LABEL}> '\n", 300);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    let top = &rows[1];
    let indent = top.chars().take_while(|c| *c == ' ').count();
    let inner = top.trim_start().chars().count() - 2;
    // The prompt is 12 columns once `${GOH_PROMPT_LABEL}` is expanded, and `git ` 4 more: the word starts at column 16.
    assert_eq!(indent, 16.min(99 - inner - 2), "{rows:?}");
    assert_eq!(indent, 16, "the expanded prompt, not its source text, sets the column: {rows:?}");
}

#[test]
fn a_command_in_a_prompt_is_not_run_again_and_counts_as_no_width() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    let counter = fx.path("prompt-runs");
    sh.send(&format!("COLUMNS=100; setopt prompt_subst; PS1='$(echo . >> {})READY> '\n", counter.display()), 500);
    let runs = |n: &str| fs::read_to_string(&counter).map(|s| s.lines().count().to_string()).unwrap_or_else(|_| n.into());
    let before = runs("0");
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    sh.send("\x1b[B", 300);
    let after_move = menu(&fx, &mut sh).0;
    let indent = rows[1].chars().take_while(|c| *c == ' ').count();
    assert_eq!(indent, 4, "anchored at `git ` as if the prompt had no width: {rows:?}");
    assert_eq!(after_move[1].chars().take_while(|c| *c == ' ').count(), 4, "{after_move:?}");
    assert_eq!(runs("0"), before, "the menu never ran the prompt's command (ran {before} before): {rows:?}");
    assert_eq!(before, "1", "the prompt ran once when it was drawn");
}

#[test]
fn the_boxs_right_border_follows_screen_cells_not_characters() {
    let fx = Fixture::new(require_env!(), false);
    fs::write(fx.path("specs/git.json"), serde_json::json!({"schemaVersion":1,"name":"git","subcommands":[{"name":"checkout","description":"日本語の説明です"}]}).to_string()).unwrap();
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    let cells = |r: &str| r.chars().map(|c| if ('\u{2e80}'..='\u{a4cf}').contains(&c) { 2 } else { 1 }).sum::<usize>();
    let top = cells(&rows[1]);
    assert!(rows[2..].iter().filter(|r| r.contains('│') || r.contains('╰')).all(|r| cells(r) == top), "every row ends in the same screen column {top}: {rows:?}");
}

/// Offset, from the start of the line, of the first character of dumped row `i` (row 0 is the ghost text) when the
/// line is `from` characters long: the ghost, a newline, then every row and its newline.
fn row_offset(rows: &[String], i: usize, from: usize) -> usize {
    from + rows[0].chars().count() + 1 + rows[1..i].iter().map(|r| r.chars().count() + 1).sum::<usize>()
}

/// The styles of the highlights spanning exactly `start..end` in a dumped `region_highlight`.
fn styles_at(h: &str, start: usize, end: usize) -> impl Iterator<Item = String> + '_ {
    let key = format!("P{start} {end} ");
    h.split(';').filter_map(move |e| e.strip_prefix(key.as_str()).map(|st| st.to_string()))
}

#[test]
fn the_box_keeps_its_shape_without_colour() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "box"), ("NO_COLOR", "1")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, h) = menu(&fx, &mut sh);
    assert!(rows[1].trim_start().starts_with('╭') && rows.iter().any(|r| r.contains("│❯ ↳ checkout")), "{rows:?}");
    // The first highlight is the ghost text on the line itself, which keeps its dim colour; the box adds none.
    let box_styles = h.split(';').skip(1).collect::<Vec<_>>().join(";");
    assert!(!box_styles.contains("bg=") && !box_styles.contains("fg="), "no colours in the box under NO_COLOR: {h}");
    assert!(box_styles.contains("bold"), "the selected row stays bold: {h}");
}

#[test]
fn flat_style_keeps_the_rows_flush_left() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "flat")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[1], SELECTED, "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains('╭')), "{rows:?}");
}

#[test]
fn the_box_moves_left_to_stay_inside_a_narrow_terminal() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    sh.send("COLUMNS=30\n", 300);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    let top = &rows[1];
    let indent = top.chars().take_while(|c| *c == ' ').count();
    // The word starts at column 11, but a box this wide would end past column 29: it slides left, never wraps.
    assert!(indent < 11, "{rows:?}");
    assert!(rows[1..].iter().all(|r| r.chars().count() <= 29), "every row fits the 30 columns: {rows:?}");
    assert!(top.trim_start().starts_with('╭') && top.ends_with('╮'), "{rows:?}");
}

#[test]
fn the_selection_highlight_follows_the_selected_row_when_the_window_scrolls() {
    let fx = Fixture::new(require_env!(), false);
    let subcommands: Vec<_> = (0..12).map(|i| serde_json::json!({"name": format!("item-{i:02}")})).collect();
    fs::write(fx.path("specs/git.json"), serde_json::json!({"schemaVersion":1,"name":"git","subcommands":subcommands}).to_string()).unwrap();
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "box"), ("GOH_LIMIT", "2")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git ", 400);
    sh.send(&"\x1b[B".repeat(3), 400);
    // Selection 4 of 12 with two rows showing: the window starts at item 3, so the selected row is the second.
    let (rows, h) = menu(&fx, &mut sh);
    assert!(rows[2].contains("│  ↳ item-02") && rows[3].contains("│❯ ↳ item-03"), "{rows:?}");
    let indent = rows[1].chars().take_while(|c| *c == ' ').count();
    let inner = rows[1].trim_start().chars().count() - 2;
    let (first, second) = (row_offset(&rows, 2, 4) + indent + 1, row_offset(&rows, 3, 4) + indent + 1);
    assert!(styles_at(&h, second, second + inner).any(|st| st.starts_with("bg=")), "the selected fill is on the second row (P{second}): {h}");
    assert!(!styles_at(&h, first, first + inner).any(|st| st.starts_with("bg=")), "and not on the first (P{first}): {h}");
    assert!(styles_at(&h, second, second + 1).any(|st| st.contains("bold")), "the bold `❯` is on the second row: {h}");
}

#[test]
fn the_box_stays_under_the_word_when_the_selection_moves_onto_a_history_line() {
    let fx = Fixture::new(require_env!(), false);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    fs::write(fx.path(".zsh_history"), format!(": {}:0;git commit -m wip\n: {}:0;git checkout -b done-once\n", now - 300, now - 200)).unwrap();
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_RECORD", "on"), ("GOH_INLINE_STYLE", "box")], true);
    menu_dump(&fx, &mut sh);
    // 50 columns keep the box narrow enough that it is never pushed left: only the anchor decides its column.
    sh.send("COLUMNS=50\n", 300);
    sh.send("git c", 1500);
    let mut on_history = false;
    for step in 0..4 {
        let (rows, _) = menu(&fx, &mut sh);
        let indent = rows[1].chars().take_while(|c| *c == ' ').count();
        // `READY> git ` is 11 columns, and a history line starts at the beginning of the line (column 7).
        assert_eq!(indent, 11, "step {step}: {rows:?}");
        on_history |= rows.iter().any(|r| r.contains("│❯ ↺ git commit -m wip"));
        sh.send("\x1b[B", 400);
    }
    assert!(on_history, "the selection did reach the history line");
    kill_socket_holder(&fx.socket);
}

#[test]
fn a_slow_docker_fills_the_open_menu_without_another_key() {
    let fx = Fixture::new(require_env!(), false);
    let bin = fx.path("slowbin");
    fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nsleep 0.6\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'\n").unwrap();
    fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let goh_dir = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap().display().to_string();
    let path = format!("{}:{goh_dir}:/usr/bin:/bin", bin.display());
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("PATH", &path)], true);
    menu_dump(&fx, &mut sh);
    sh.send("docker exec ", 1500); // the first key also starts the daemon
    // No key after this: the answer must arrive through the adapter's own re-ask.
    sh.drain(Duration::from_millis(1500), Duration::from_secs(3));
    let (rows, _) = menu(&fx, &mut sh);
    kill_socket_holder(&fx.socket);
    assert!(rows.iter().any(|r| r.contains("web-1")), "the late container shows by itself: {rows:?}");
}

/// A shell whose `docker` waits `delay` seconds before answering with one container, `web-1`.
fn slow_docker_shell(fx: &Fixture, delay: &str) -> Pty {
    let bin = fx.path("slowbin");
    fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, format!("#!/bin/sh\nsleep {delay}\necho '{{\"Names\":\"web-1\",\"Image\":\"nginx\"}}'\n")).unwrap();
    fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let goh_dir = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap().display().to_string();
    let path = format!("{}:{goh_dir}:/usr/bin:/bin", bin.display());
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("PATH", &path)], true);
    menu_dump(fx, &mut sh);
    sh
}

/// Presses Ctrl-C and waits (up to 3 s) for the fresh `READY> ` prompt.
fn ctrl_c_returns_the_prompt(sh: &mut Pty) -> bool {
    sh.write_all_keys(b"\x03");
    let (start, mut out) = (Instant::now(), String::new());
    while start.elapsed() < Duration::from_secs(3) && !out.contains("READY> ") {
        out.push_str(&String::from_utf8_lossy(&sh.drain(Duration::from_millis(20), Duration::from_millis(100))));
    }
    out.contains("READY> ")
}

#[test]
fn ctrl_c_while_a_late_answer_is_pending_aborts_the_line_at_once() {
    let locale = require_env!();
    // The adapter asks again every ~0.4 s while a program is slow, and each ask holds the shell for a moment: a
    // Ctrl-C landing in that moment used to be acted on only at the next key. A fresh shell each time (the daemon
    // remembers how long a program has been slow) and a Ctrl-C a few seconds in make that about a third of the tries.
    for _ in 0..6 {
        let fx = Fixture::new(locale.clone(), false);
        let mut sh = slow_docker_shell(&fx, "20");
        sh.send("docker exec ", 800);
        sh.drain(Duration::from_millis(1800), Duration::from_millis(1800));
        let back = ctrl_c_returns_the_prompt(&mut sh);
        kill_socket_holder(&fx.socket);
        assert!(back, "Ctrl-C was swallowed until the next key");
    }
}

#[test]
fn a_new_prompt_starts_the_late_answer_count_again() {
    let fx = Fixture::new(require_env!(), false);
    let mut sh = slow_docker_shell(&fx, "20");
    sh.send(&format!("latew() {{ print -r -- \"TRIES=$_goh_late_tries LINE[$_goh_late_line]\" >> {}; }}; zle -N latew; bindkey '^Xl' latew\n", fx.path("late.txt").display()), 300);
    sh.send("docker exec ", 800);
    sh.drain(Duration::from_millis(3500), Duration::from_secs(4));
    let asked = fx.state_file(&mut sh, "\x18l", "late.txt");
    assert!(!asked.starts_with("TRIES=0 "), "the line has been re-asked a few times: {asked}");
    // Without the reset, the same line typed at a later prompt would carry the count over and give up at once once
    // it had passed 16.
    assert!(ctrl_c_returns_the_prompt(&mut sh), "Ctrl-C brings the prompt back");
    let fresh = fx.state_file(&mut sh, "\x18l", "late.txt");
    kill_socket_holder(&fx.socket);
    assert_eq!(fresh, "TRIES=0 LINE[]", "a new prompt counts again from nothing");
}

#[test]
fn a_late_answer_does_not_reopen_a_menu_closed_with_esc() {
    let fx = Fixture::new(require_env!(), false);
    let mut sh = slow_docker_shell(&fx, "2.5");
    sh.send("docker exec ", 800);
    sh.send("\x1b", 200);
    sh.drain(Duration::from_millis(1500), Duration::from_secs(6));
    let state = fx.state(&mut sh);
    let (rows, _) = menu(&fx, &mut sh);
    kill_socket_holder(&fx.socket);
    assert!(state.contains("on=0"), "Esc keeps the menu closed: {state}");
    assert!(!rows.iter().any(|r| r.contains("web-1")), "{rows:?}");
}

#[test]
fn a_late_answer_leaves_the_users_selection_alone() {
    let fx = Fixture::new(require_env!(), false);
    let mut sh = slow_docker_shell(&fx, "2.5");
    sh.send("docker exec ", 800);
    sh.send("\x1b[B", 200);
    sh.drain(Duration::from_millis(1500), Duration::from_secs(6));
    let state = fx.state(&mut sh);
    let (rows, _) = menu(&fx, &mut sh);
    kill_socket_holder(&fx.socket);
    assert!(state.contains("on=1 sel=2"), "the highlight stays on the second row: {state}");
    assert!(rows.iter().any(|r| r.starts_with("❯ ") && r.contains("-d")), "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains("web-1") || r.contains('…')), "nothing reshuffled after the user moved: {rows:?}");
}
