//! `goh doctor [--fix]`: what works, what does not and why. The checks are pure over `Inputs`
//! (tested); `gather` reads the real system and `fix` acts on what a check marked fixable.

use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub name: &'static str,
    pub detail: String,
    pub fix: Option<Fix>,
}

#[derive(Debug, Default)]
pub struct Inputs {
    /// `goh`, `gohd`, `goh-popup`: where each was found.
    pub binaries: Vec<(&'static str, Option<PathBuf>)>,
    pub daemon_running: bool,
    pub specs: usize,
    pub spec_warnings: Vec<String>,
    pub term_program: Option<String>,
    pub shell: Option<String>,
    pub remote: bool,
    pub multiplexer: Option<&'static str>,
    /// `None`: no such permission on this platform.
    pub accessibility: Option<bool>,
    pub zoxide: bool,
    /// `None`: no config file. `Some(Err)`: the problem with it.
    pub config: Option<Result<(), String>>,
}

const OVERLAY_TERMINALS: [&str; 4] = ["iTerm.app", "Apple_Terminal", "vscode", "claude-desktop"];
const ADAPTERS: [&str; 7] = ["zsh", "bash", "fish", "pwsh", "nu", "elvish", "xonsh"];

fn check(level: Level, name: &'static str, detail: impl Into<String>, fix: Option<Fix>) -> Check {
    Check { level, name, detail: detail.into(), fix }
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
    out.push(if i.daemon_running {
        check(Level::Ok, "daemon", "gohd is running", None)
    } else {
        check(Level::Warn, "daemon", "gohd is not running (the first Tab starts it)", Some(Fix::StartDaemon))
    });
    out.push(check(Level::Ok, "specs", format!("{} commands loaded", i.specs), None));
    for w in &i.spec_warnings {
        out.push(check(Level::Warn, "specs", w.clone(), None));
    }
    out.push(match i.shell.as_deref() {
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
    out.push(if i.zoxide {
        check(Level::Info, "zoxide", "found: `cd` suggestions use it, with goh's own history as backup", None)
    } else {
        check(Level::Info, "zoxide", "not installed: `cd` suggestions use goh's own history", None)
    });
    out
}

fn find_binary(name: &str, beside: Option<&Path>) -> Option<PathBuf> {
    let beside = beside.map(|d| d.join(name)).filter(|p| p.is_file());
    beside.or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
    })
}

pub fn gather() -> Inputs {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let (loaded, spec_warnings) = crate::specs::load(&crate::specs::default_dir());
    let env_set = |k| std::env::var_os(k).is_some();
    Inputs {
        binaries: ["goh", "gohd", "goh-popup"].into_iter().map(|n| (n, find_binary(n, exe_dir.as_deref()))).collect(),
        daemon_running: std::os::unix::net::UnixStream::connect(crate::paths::socket_path()).is_ok(),
        specs: loaded.len(),
        spec_warnings,
        term_program: std::env::var("TERM_PROGRAM").ok(),
        shell: std::env::var("SHELL").ok().and_then(|s| Path::new(&s).file_name().map(|n| n.to_string_lossy().into_owned())),
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
            daemon_running: true,
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
}
