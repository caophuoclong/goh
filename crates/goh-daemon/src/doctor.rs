//! `goh doctor [--fix]`: what works, what does not and why. The checks are pure over `Inputs`
//! (tested); `gather` reads the real system and `fix` acts on what a check marked fixable.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    OpenAccessibility,
    StartDaemon,
    RestartDaemon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub name: &'static str,
    pub detail: String,
    pub fix: Option<Fix>,
}

/// Has the adapter been loaded by the shell that ran `goh doctor`? The adapters export `GOH_ADAPTER=<shell>:<pid>`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    /// The variable names this very shell (its pid is our parent's).
    Loaded,
    /// Set, but by another shell: doctor was run from a subshell or a script, or the variable was inherited.
    Inherited,
    #[default]
    Absent,
}

/// What `goh doctor --test` saw when it asked the daemon for `git chec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundTrip {
    Offered { millis: u128 },
    Nothing { millis: u128 },
    Failed(String),
}

/// The line `--test` completes, and the word it expects back.
pub const TEST_LINE: &str = "git chec";
const TEST_EXPECT: &str = "checkout";
/// The adapters give up on the daemon after 200 ms, so an answer near that is as good as none.
const SLOW_MILLIS: u128 = 100;

#[derive(Debug, Default)]
pub struct Inputs {
    /// Version of this `goh`.
    pub version: String,
    /// `goh`, `gohd`, `goh-popup`: where each was found.
    pub binaries: Vec<(&'static str, Option<PathBuf>)>,
    pub daemon_running: bool,
    /// What the running daemon says it is; `None` when it did not answer (one from before `Hello`).
    pub daemon_version: Option<String>,
    pub specs: usize,
    pub spec_warnings: Vec<String>,
    pub term_program: Option<String>,
    pub shell: Option<String>,
    pub adapter: Adapter,
    /// The shell's startup file and whether it holds the block `goh setup` writes. `None`: no such file for this shell.
    pub rc: Option<(PathBuf, bool)>,
    pub remote: bool,
    pub multiplexer: Option<&'static str>,
    /// `None`: no such permission on this platform.
    pub accessibility: Option<bool>,
    pub zoxide: bool,
    pub auto_specs: bool,
    /// Panics among the last lines of the daemon's log.
    pub recent_panics: usize,
    /// `Some` only for `--test`.
    pub round_trip: Option<RoundTrip>,
    /// `None`: no config file. `Some(Err)`: the problem with it.
    pub config: Option<Result<(), String>>,
}

const OVERLAY_TERMINALS: [&str; 4] = ["iTerm.app", "Apple_Terminal", "vscode", "claude-desktop"];
const ADAPTERS: [&str; 7] = ["zsh", "bash", "fish", "pwsh", "nu", "elvish", "xonsh"];

fn check(level: Level, name: &'static str, detail: impl Into<String>, fix: Option<Fix>) -> Check {
    Check { level, name, detail: detail.into(), fix }
}

/// The shells `goh setup` writes a block for: is the adapter in this shell, and is it in the startup file?
fn adapter_check(shell: &str, i: &Inputs) -> Check {
    let rc = i.rc.as_ref();
    let in_rc = rc.is_some_and(|(_, has)| *has);
    let file = rc.map_or_else(|| "its startup file".to_string(), |(p, _)| p.display().to_string());
    match i.adapter {
        Adapter::Loaded if in_rc => check(Level::Ok, "shell", format!("{shell}: adapter loaded here, from {file}"), None),
        Adapter::Loaded => check(Level::Ok, "shell", format!("{shell}: adapter loaded here (by hand: {file} has no `goh setup` block)"), None),
        Adapter::Inherited => check(Level::Info, "shell", format!("{shell}: goh is loaded in a parent shell; run `goh doctor` at your prompt to check this one"), None),
        Adapter::Absent if in_rc => check(Level::Warn, "shell", format!("{shell}: {file} loads goh, but this shell started without it: open a new terminal or run `exec $SHELL`"), None),
        Adapter::Absent => check(Level::Warn, "shell", format!("{shell}: the adapter is not loaded and {file} has no goh block: run `goh setup`, then open a new terminal"), None),
    }
}

pub fn evaluate(i: &Inputs) -> Vec<Check> {
    let mut out = Vec::new();
    for (name, found) in &i.binaries {
        out.push(match (found, *name) {
            (Some(p), _) => check(Level::Ok, "binary", format!("{name}: {}", p.display()), None),
            (None, "goh-popup") => check(Level::Warn, "binary", "goh-popup not found: menus are drawn inline only", None),
            (None, _) => check(Level::Fail, "binary", format!("{name} not found on PATH or next to goh: completion cannot work"), None),
        });
    }
    out.push(match (i.daemon_running, i.daemon_version.as_deref()) {
        (false, _) => check(Level::Warn, "daemon", "gohd is not running (the first Tab starts it)", Some(Fix::StartDaemon)),
        (true, Some(v)) if v == i.version => check(Level::Ok, "daemon", format!("gohd {v} is running"), None),
        (true, Some(v)) => check(Level::Warn, "daemon", format!("gohd {v} is running but goh is {}: an upgrade left the old daemon in place", i.version), Some(Fix::RestartDaemon)),
        (true, None) => check(Level::Warn, "daemon", format!("the running gohd is older than goh {} (it does not report a version)", i.version), Some(Fix::RestartDaemon)),
    });
    if i.recent_panics > 0 {
        out.push(check(Level::Warn, "daemon log", format!("{} recent panic(s) in gohd's log: `goh logs` shows where", i.recent_panics), None));
    }
    out.push(check(Level::Ok, "specs", format!("{} commands loaded", i.specs), None));
    for w in &i.spec_warnings {
        out.push(check(Level::Warn, "specs", w.clone(), None));
    }
    out.push(match i.shell.as_deref() {
        Some(s) if crate::setup::supported(s) => adapter_check(s, i),
        Some(s) if ADAPTERS.contains(&s) => check(Level::Info, "shell", format!("{s}: load its adapter with `goh setup` (or `goh init {s}` from your startup file)"), None),
        Some(s) => check(Level::Warn, "shell", format!("{s} has no goh adapter"), None),
        None => check(Level::Warn, "shell", "$SHELL is not set", None),
    });
    let term = i.term_program.as_deref().unwrap_or("unknown");
    if i.remote {
        out.push(check(Level::Info, "terminal", "over ssh: the menu is drawn inline (the popup cannot know where the caret is)", None));
    } else if let Some(m) = i.multiplexer {
        out.push(check(Level::Info, "terminal", format!("inside {m}: the menu is drawn inline"), None));
    } else if OVERLAY_TERMINALS.contains(&term) {
        out.push(check(Level::Ok, "terminal", format!("{term}: popup supported"), None));
    } else {
        out.push(check(Level::Info, "terminal", format!("{term}: no verified caret lookup yet; the menu is drawn inline"), None));
    }
    // Only terminals with a verified caret use the permission, so only they can be told they lack it.
    match i.accessibility {
        Some(false) if OVERLAY_TERMINALS.contains(&term) => out.push(check(
            Level::Warn,
            "accessibility",
            format!("{term} is not allowed to use Accessibility: the popup is disabled (inline menu still works)"),
            Some(Fix::OpenAccessibility),
        )),
        Some(false) => out.push(check(Level::Info, "accessibility", format!("{term} has no Accessibility permission (only needed for the popup)"), Some(Fix::OpenAccessibility))),
        Some(true) => out.push(check(Level::Ok, "accessibility", format!("{term} may use Accessibility"), None)),
        None => {}
    }
    out.push(match &i.config {
        None => check(Level::Info, "config", "no config file: defaults are in effect (`goh config` creates one)", None),
        Some(Ok(())) => check(Level::Ok, "config", "config file is valid", None),
        Some(Err(e)) => check(Level::Warn, "config", format!("{e} (run `goh config` to fix it)"), None),
    });
    out.push(if i.auto_specs {
        check(Level::Info, "auto specs", "on: a command with no spec gets one read from its --help", None)
    } else {
        check(Level::Info, "auto specs", "off: a command with no spec gets no suggestions (`goh spec gen <command>` writes one, or set completion.autoSpecs in `goh config`)", None)
    });
    out.push(if i.zoxide {
        check(Level::Info, "zoxide", "found: `cd` suggestions use it, with goh's own history as backup", None)
    } else {
        check(Level::Info, "zoxide", "not installed: `cd` suggestions use goh's own history", None)
    });
    match &i.round_trip {
        None => {}
        Some(RoundTrip::Offered { millis }) if *millis <= SLOW_MILLIS => {
            out.push(check(Level::Ok, "round trip", format!("`{TEST_LINE}` offered `{TEST_EXPECT}` in {millis} ms"), None));
        }
        Some(RoundTrip::Offered { millis }) => {
            out.push(check(Level::Warn, "round trip", format!("`{TEST_LINE}` took {millis} ms; the adapters give up at 200 ms, so menus may not appear"), None));
        }
        Some(RoundTrip::Nothing { millis }) => out.push(check(
            Level::Warn,
            "round trip",
            format!("gohd answered in {millis} ms but offered nothing for `{TEST_LINE}` (a `git` spec of yours may replace the bundled one)"),
            None,
        )),
        Some(RoundTrip::Failed(e)) => out.push(check(Level::Fail, "round trip", format!("gohd did not answer `{TEST_LINE}`: {e}"), None)),
    }
    out
}

/// Ask the running daemon to complete [`TEST_LINE`], as an adapter would.
pub fn round_trip(sock: &Path) -> RoundTrip {
    let start = std::time::Instant::now();
    let session = format!("doctor-{}", std::process::id());
    let reply = crate::client::menu(sock, &session, "/", TEST_LINE, TEST_LINE.len(), 20, false);
    let millis = start.elapsed().as_millis();
    match reply {
        Ok(items) if items.iter().any(|i| i.label == TEST_EXPECT) => RoundTrip::Offered { millis },
        Ok(_) => RoundTrip::Nothing { millis },
        Err(e) => RoundTrip::Failed(e.to_string()),
    }
}

/// What a bug report needs besides the checks. Nothing here comes from what you typed.
pub struct Environment {
    pub os: String,
    pub terminal: String,
    pub shell: String,
}

/// A report to paste into an issue: versions, the checks, and no command text; your home folder shows as `~`.
pub fn report(version: &str, env: &Environment, checks: &[Check], home: Option<&str>) -> String {
    let mut out = format!("goh {version} on {}\nshell: {}\nterminal: {}\n\n", env.os, env.shell, env.terminal);
    for c in checks {
        out.push_str(&format!("{}  {:<14} {}\n", label(c.level), c.name, c.detail));
    }
    match home {
        Some(h) if h.len() > 1 => out.replace(h, "~"),
        _ => out,
    }
}

/// Real system facts for [`report`]; runs the shell once for its version.
pub fn environment() -> Environment {
    let shell_path = std::env::var("SHELL").ok();
    let name = shell_path.as_deref().and_then(|s| Path::new(s).file_name()).map(|n| n.to_string_lossy().into_owned());
    let version = match (&shell_path, name.as_deref()) {
        (Some(path), Some("zsh" | "bash" | "fish")) => std::process::Command::new(path)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|t| t.lines().next().map(str::to_string)),
        _ => None,
    };
    Environment {
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        terminal: std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unknown".into()),
        shell: version.or(name).unwrap_or_else(|| "unknown".into()),
    }
}

fn find_binary(name: &str, beside: Option<&Path>) -> Option<PathBuf> {
    let beside = beside.map(|d| d.join(name)).filter(|p| p.is_file());
    beside.or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
    })
}

/// Reads `GOH_ADAPTER` (`<shell>:<pid>`, exported by the adapters) against the shell that started us.
fn adapter_state(var: Option<&str>, parent: u32) -> Adapter {
    match var.and_then(|v| v.rsplit_once(':')).map(|(_, pid)| pid.parse::<u32>()) {
        Some(Ok(pid)) if pid == parent => Adapter::Loaded,
        Some(_) => Adapter::Inherited,
        None => Adapter::Absent,
    }
}

/// The startup file `goh setup` would write for this shell, and whether it already holds the block.
fn startup_file(shell: &str) -> Option<(PathBuf, bool)> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let path = env("GOH_RC").or_else(|| {
        crate::setup::rc_path(shell, env("HOME")?.as_path(), env("ZDOTDIR").as_deref(), env("XDG_CONFIG_HOME").as_deref(), cfg!(target_os = "macos"))
    })?;
    let has = std::fs::read_to_string(&path).is_ok_and(|t| crate::setup::has_block(&t));
    Some((path, has))
}

pub fn gather() -> Inputs {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let (loaded, spec_warnings) = crate::specs::load(&crate::specs::default_dir());
    let env_set = |k| std::env::var_os(k).is_some();
    let sock = crate::paths::socket_path();
    let daemon_running = std::os::unix::net::UnixStream::connect(&sock).is_ok();
    // SAFETY: getppid has no preconditions and cannot fail.
    let parent = unsafe { libc::getppid() } as u32;
    let shell = std::env::var("SHELL").ok().and_then(|s| Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned()));
    Inputs {
        adapter: adapter_state(std::env::var("GOH_ADAPTER").ok().as_deref(), parent),
        rc: shell.as_deref().filter(|s| crate::setup::supported(s)).and_then(startup_file),
        auto_specs: crate::autospec::enabled(),
        round_trip: None,
        recent_panics: crate::log::tail(&crate::log::path(), 200).map_or(0, |t| t.lines().filter(|l| l.contains(" panic at ")).count()),
        binaries: ["goh", "gohd", "goh-popup"].into_iter().map(|n| (n, find_binary(n, exe_dir.as_deref()))).collect(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        daemon_running,
        daemon_version: if daemon_running { crate::client::hello(&sock).ok().map(|(v, _)| v) } else { None },
        specs: loaded.len(),
        spec_warnings,
        term_program: std::env::var("TERM_PROGRAM").ok(),
        shell,
        remote: env_set("SSH_CONNECTION") || env_set("SSH_TTY"),
        multiplexer: if env_set("TMUX") { Some("tmux") } else if env_set("STY") { Some("screen") } else { None },
        accessibility: goh_anchor::accessibility_trusted(),
        zoxide: find_binary("zoxide", None).is_some(),
        config: crate::config::path().ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| crate::config::validate(&t)),
    }
}

/// Perform one fix; the returned text says what happened.
pub fn fix(f: Fix) -> String {
    match f {
        Fix::RestartDaemon => {
            let sock = crate::paths::socket_path();
            match crate::control::stop(&sock).and_then(|_| crate::control::spawn()) {
                Ok(()) if crate::control::wait_until_up(&sock, Duration::from_secs(2)) => "restarted gohd".into(),
                Ok(()) => "stopped gohd but it did not come back within 2 s".into(),
                Err(e) => format!("could not restart gohd: {e}"),
            }
        }
        Fix::StartDaemon => match crate::control::spawn() {
            Ok(()) => "started gohd".into(),
            Err(e) => format!("could not start gohd: {e}"),
        },
        Fix::OpenAccessibility => {
            goh_anchor::request_accessibility();
            let opened = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                .status()
                .is_ok_and(|s| s.success());
            if opened {
                "opened System Settings > Privacy & Security > Accessibility: switch on your terminal app (use + if it is missing), then quit and reopen it".into()
            } else {
                "could not open System Settings: go to Privacy & Security > Accessibility and switch on your terminal app".into()
            }
        }
    }
}

pub fn label(l: Level) -> &'static str {
    match l {
        Level::Ok => "ok  ",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Fail => "FAIL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Inputs {
        Inputs {
            binaries: vec![("goh", Some("/x/goh".into())), ("gohd", Some("/x/gohd".into())), ("goh-popup", Some("/x/goh-popup".into()))],
            version: "1.2.3".into(),
            daemon_running: true,
            daemon_version: Some("1.2.3".into()),
            adapter: Adapter::Loaded,
            rc: Some(("/home/u/.zshrc".into(), true)),
            specs: 30,
            term_program: Some("iTerm.app".into()),
            shell: Some("zsh".into()),
            accessibility: Some(true),
            ..Default::default()
        }
    }

    fn worst(c: &[Check]) -> Level {
        [Level::Fail, Level::Warn, Level::Info, Level::Ok].into_iter().find(|l| c.iter().any(|x| x.level == *l)).unwrap()
    }

    #[test]
    fn a_healthy_setup_has_nothing_to_fix() {
        let c = evaluate(&base());
        assert_eq!(worst(&c), Level::Info);
        assert!(c.iter().all(|x| x.fix.is_none()));
    }

    #[test]
    fn a_missing_permission_on_a_popup_terminal_is_fixable() {
        let c = evaluate(&Inputs { accessibility: Some(false), ..base() });
        let a = c.iter().find(|x| x.name == "accessibility").unwrap();
        assert_eq!((a.level, a.fix), (Level::Warn, Some(Fix::OpenAccessibility)));
    }

    #[test]
    fn a_terminal_without_a_popup_is_not_nagged_about_the_permission() {
        let c = evaluate(&Inputs { accessibility: Some(false), term_program: Some("WezTerm".into()), ..base() });
        assert_eq!(c.iter().find(|x| x.name == "accessibility").unwrap().level, Level::Info);
        assert_eq!(c.iter().find(|x| x.name == "terminal").unwrap().level, Level::Info);
    }

    #[test]
    fn missing_core_binaries_fail_and_a_stopped_daemon_can_be_started() {
        let mut i = base();
        i.binaries[1].1 = None;
        i.binaries[2].1 = None;
        i.daemon_running = false;
        let c = evaluate(&i);
        assert_eq!(c.iter().filter(|x| x.level == Level::Fail).count(), 1);
        assert!(c.iter().any(|x| x.level == Level::Warn && x.detail.contains("goh-popup")));
        assert_eq!(c.iter().find(|x| x.name == "daemon").unwrap().fix, Some(Fix::StartDaemon));
    }

    fn named<'a>(c: &'a [Check], name: &str) -> &'a Check {
        c.iter().find(|x| x.name == name).unwrap()
    }

    #[test]
    fn a_shell_that_started_without_the_adapter_is_told_why() {
        let c = evaluate(&Inputs { adapter: Adapter::Absent, ..base() });
        let s = named(&c, "shell");
        assert_eq!(s.level, Level::Warn);
        assert!(s.detail.contains("exec $SHELL"), "{}", s.detail);
        let c = evaluate(&Inputs { adapter: Adapter::Absent, rc: Some(("/home/u/.zshrc".into(), false)), ..base() });
        assert!(named(&c, "shell").detail.contains("goh setup"));
        let c = evaluate(&Inputs { adapter: Adapter::Absent, rc: None, ..base() });
        assert!(named(&c, "shell").detail.contains("goh setup"));
    }

    #[test]
    fn an_adapter_loaded_by_hand_or_in_a_parent_is_not_a_warning() {
        let c = evaluate(&Inputs { rc: Some(("/home/u/.zshrc".into(), false)), ..base() });
        assert_eq!(named(&c, "shell").level, Level::Ok);
        let c = evaluate(&Inputs { adapter: Adapter::Inherited, ..base() });
        assert_eq!(named(&c, "shell").level, Level::Info);
    }

    #[test]
    fn the_adapter_variable_is_matched_against_the_parent_shell() {
        assert_eq!(adapter_state(Some("zsh:4242"), 4242), Adapter::Loaded);
        assert_eq!(adapter_state(Some("zsh:4242"), 1), Adapter::Inherited);
        assert_eq!(adapter_state(Some("garbage"), 1), Adapter::Absent);
        assert_eq!(adapter_state(None, 1), Adapter::Absent);
    }

    #[test]
    fn a_daemon_from_an_older_version_is_restartable() {
        let c = evaluate(&Inputs { daemon_version: Some("1.1.0".into()), ..base() });
        assert_eq!((named(&c, "daemon").level, named(&c, "daemon").fix), (Level::Warn, Some(Fix::RestartDaemon)));
        let c = evaluate(&Inputs { daemon_version: None, ..base() });
        assert_eq!(named(&c, "daemon").fix, Some(Fix::RestartDaemon));
        let c = evaluate(&base());
        assert_eq!(named(&c, "daemon").level, Level::Ok);
    }

    #[test]
    fn the_round_trip_is_judged_by_what_came_back_and_how_fast() {
        let level = |r| named(&evaluate(&Inputs { round_trip: Some(r), ..base() }), "round trip").level;
        assert_eq!(level(RoundTrip::Offered { millis: 12 }), Level::Ok);
        assert_eq!(level(RoundTrip::Offered { millis: 150 }), Level::Warn);
        assert_eq!(level(RoundTrip::Nothing { millis: 5 }), Level::Warn);
        assert_eq!(level(RoundTrip::Failed("timed out".into())), Level::Fail);
        assert!(evaluate(&base()).iter().all(|c| c.name != "round trip"));
    }

    #[test]
    fn recent_panics_and_auto_specs_are_reported() {
        let c = evaluate(&Inputs { recent_panics: 2, ..base() });
        assert_eq!(named(&c, "daemon log").level, Level::Warn);
        assert!(evaluate(&base()).iter().all(|c| c.name != "daemon log"));
        assert!(named(&evaluate(&base()), "auto specs").detail.starts_with("off"));
        assert!(named(&evaluate(&Inputs { auto_specs: true, ..base() }), "auto specs").detail.starts_with("on"));
    }

    #[test]
    fn a_report_hides_the_home_folder_and_holds_no_command_text() {
        let env = Environment { os: "macos aarch64".into(), terminal: "iTerm.app".into(), shell: "zsh 5.9".into() };
        let mut i = base();
        i.binaries[0].1 = Some("/Users/zorblax/.local/bin/goh".into());
        let text = report("1.2.3", &env, &evaluate(&i), Some("/Users/zorblax"));
        assert!(text.starts_with("goh 1.2.3 on macos aarch64\nshell: zsh 5.9\nterminal: iTerm.app"));
        assert!(text.contains("~/.local/bin/goh"));
        assert!(!text.contains("zorblax"));
        // No home known: nothing is replaced, and a bare "/" must not turn every slash into "~".
        assert!(report("1.2.3", &env, &evaluate(&i), Some("/")).contains("/Users/zorblax/.local/bin/goh"));
    }
}
