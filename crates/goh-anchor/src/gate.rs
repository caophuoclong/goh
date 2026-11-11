//! Conditions under which the measured behaviour can be trusted: the shell is hosted by the
//! focused app, locally, with no multiplexer in between.

use crate::rules::{App, Reason};

pub fn check_env(env: &dyn Fn(&str) -> Option<String>, app: App) -> Result<(), Reason> {
    if env("TMUX").is_some() {
        return Err(Reason::Multiplexer("tmux"));
    }
    if env("STY").is_some() {
        return Err(Reason::Multiplexer("screen"));
    }
    if env("SSH_CONNECTION").is_some() || env("SSH_TTY").is_some() {
        return Err(Reason::Remote);
    }
    match env("TERM_PROGRAM") {
        Some(p) if p == app.term_program() => Ok(()),
        other => Err(Reason::TermProgramMismatch { expected: app.term_program(), found: other.unwrap_or_default() }),
    }
}

/// Windows: the shell must run in Windows Terminal (it sets `WT_SESSION` in every pane, and passes it into WSL), on
/// this machine, with no multiplexer. A popup started from WSL gets these variables through `WSLENV`.
pub fn check_windows_env(env: &dyn Fn(&str) -> Option<String>) -> Result<(), Reason> {
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    if set("TMUX") {
        return Err(Reason::Multiplexer("tmux"));
    }
    if set("STY") {
        return Err(Reason::Multiplexer("screen"));
    }
    if set("SSH_CONNECTION") || set("SSH_TTY") {
        return Err(Reason::Remote);
    }
    if !set("WT_SESSION") {
        return Err(Reason::Windows("the shell is not running in Windows Terminal (WT_SESSION is not set)".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn matching_local_terminal_passes() {
        assert_eq!(check_env(&env(&[("TERM_PROGRAM", "iTerm.app")]), App::ITerm2), Ok(()));
        assert_eq!(check_env(&env(&[("TERM_PROGRAM", "Apple_Terminal")]), App::Terminal), Ok(()));
    }

    #[test]
    fn the_shells_host_must_be_the_focused_app() {
        // Focus moved to iTerm2 while this shell runs in Terminal.app: the caret would be the wrong one.
        let r = check_env(&env(&[("TERM_PROGRAM", "Apple_Terminal")]), App::ITerm2);
        assert!(matches!(r, Err(Reason::TermProgramMismatch { .. })));
        assert!(matches!(check_env(&env(&[]), App::Terminal), Err(Reason::TermProgramMismatch { .. })));
    }

    #[test]
    fn multiplexers_and_ssh_are_not_verified() {
        let base = ("TERM_PROGRAM", "iTerm.app");
        assert_eq!(check_env(&env(&[base, ("TMUX", "/tmp/x")]), App::ITerm2), Err(Reason::Multiplexer("tmux")));
        assert_eq!(check_env(&env(&[base, ("STY", "1")]), App::ITerm2), Err(Reason::Multiplexer("screen")));
        assert_eq!(check_env(&env(&[base, ("SSH_CONNECTION", "a b c d")]), App::ITerm2), Err(Reason::Remote));
    }

    #[test]
    fn windows_needs_a_local_windows_terminal_pane() {
        let wt = ("WT_SESSION", "2f1b3c1e-0000-4000-8000-000000000000");
        assert_eq!(check_windows_env(&env(&[wt])), Ok(()));
        assert!(matches!(check_windows_env(&env(&[])), Err(Reason::Windows(_))));
        assert!(matches!(check_windows_env(&env(&[("WT_SESSION", "")])), Err(Reason::Windows(_))));
        assert_eq!(check_windows_env(&env(&[wt, ("TMUX", "/tmp/tmux-1000/default,1,0")])), Err(Reason::Multiplexer("tmux")));
        assert_eq!(check_windows_env(&env(&[wt, ("SSH_TTY", "/dev/pts/1")])), Err(Reason::Remote));
    }
}
