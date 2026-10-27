//! Adapter helper.
//!   goh-ctl menu     --session S --cwd D --cursor-byte N --buffer B [--limit N] [--live]
//!       --live: as-you-type; no output unless the line is a recognised command in progress
//!       -> one line per candidate: start US end US cursor US kind US label US insert US description US ranges
//!          (ranges = `s-e,s-e` byte ranges of the label that matched; may be empty)
//!          (US = 0x1f; offsets are UTF-8 bytes)
//!   goh-ctl complete --session S --cwd D --cursor-byte N --buffer B   -> results JSON
//!   goh-ctl theme [--probe] [--json]
//!       -> resolved design tokens as `key value` lines (or JSON); --probe asks the terminal for its colours
//! Exit 1 with no output on any failure; if the daemon is not running, start it for next time.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};

use goh_daemon::{client, paths, probe, wire::Reply};
use goh_theme::{parse_colorfgbg, resolve, Mode, Terminal, ThemeConfig};

fn start_daemon() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe.with_file_name("gohd"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
    }
}

/// Theme section of `$GOH_CONFIG` or `~/.config/goh/config.json`; anything unusable means `auto`.
fn theme_mode() -> Mode {
    let path = std::env::var_os("GOH_CONFIG")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/goh/config.json")));
    let cfg = path.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    cfg.and_then(|c| serde_json::from_value::<ThemeConfig>(c.get("theme")?.clone()).ok()).map(|t| t.mode()).unwrap_or_default()
}

/// Query the terminal only on a real, non-multiplexed terminal and only when no override makes it moot.
fn theme(args: Vec<String>) -> Option<()> {
    let probe = args.iter().any(|a| a == "--probe");
    let mode = theme_mode();
    let mut term = Terminal {
        colorfgbg_bg: std::env::var("COLORFGBG").ok().and_then(|v| parse_colorfgbg(&v)),
        ..Default::default()
    };
    let env_set = |k| std::env::var_os(k).is_some();
    let can_probe = probe
        && matches!(mode, Mode::Auto)
        && !env_set("TMUX")
        && !env_set("STY")
        && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(false);
    if can_probe {
        if let Ok(r) = probe::query_colors(Duration::from_millis(100)) {
            (term.fg, term.bg, term.probed) = (r.fg, r.bg, true);
        }
    }
    // iTerm2's cell height, for the overlay: it can only be asked here, between prompts, because the
    // reply is terminal input and a running line editor would take it as typing.
    let cell_height = (can_probe && std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "iTerm.app"))
        .then(goh_anchor::iterm_cell_height)
        .flatten();
    let resolved = resolve(mode, &term);
    if args.iter().any(|a| a == "--json") {
        println!("{}", serde_json::to_string_pretty(&resolved.json()).ok()?);
    } else {
        print!("{}", resolved.lines());
        if let Some(h) = cell_height {
            println!("cell-height {h}");
        }
    }
    Some(())
}

/// After starting the daemon, wait (bounded) until its socket accepts connections.
fn wait_for_daemon(sock: &std::path::Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if UnixStream::connect(sock).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn run() -> Option<()> {
    let mut args = std::env::args().skip(1);
    let sub = args.next()?;
    if sub == "theme" {
        return theme(args.collect());
    }
    let (mut session, mut cwd, mut cursor, mut buffer, mut limit, mut live) = (None, None, None, None, 8usize, false);
    while let Some(flag) = args.next() {
        if flag == "--live" {
            live = true; // a flag with no value
            continue;
        }
        let v = args.next()?;
        match flag.as_str() {
            "--session" => session = Some(v),
            "--cwd" => cwd = Some(v),
            "--cursor-byte" => cursor = Some(v.parse::<usize>().ok()?),
            "--buffer" => buffer = Some(v),
            "--limit" => limit = v.parse().ok()?,
            _ => return None,
        }
    }
    let (session, cwd, cursor, buffer) = (session?, cwd?, cursor?, buffer?);
    let sock = paths::socket_path();

    let attempt = || match sub.as_str() {
        "menu" => client::menu(&sock, &session, &cwd, &buffer, cursor, limit, live).map(|items| {
            (!items.is_empty()).then(|| {
                for i in items {
                    let e = i.edit;
                    let ranges: Vec<String> = i.matches.iter().map(|r| format!("{}-{}", r[0], r[1])).collect();
                    println!("{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}", e.start, e.end, e.cursor, i.kind, i.label, e.insert, i.description, ranges.join(","));
                }
            })
        }),
        "complete" => client::complete(&sock, &session, &cwd, &buffer, cursor, live).map(|(_, _, r)| {
            matches!(r, Reply::Results { .. }).then(|| println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default()))
        }),
        _ => Ok(None),
    };
    if !matches!(sub.as_str(), "menu" | "complete") {
        return None;
    }
    let not_running = |e: &std::io::Error| matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused);
    let mut result = attempt();
    // No daemon (never started, or it died and left a stale socket): start it, give it a moment to come
    // up, and ask once more, so the first request after a cold start is not silently lost.
    if result.as_ref().is_err_and(not_running) {
        start_daemon();
        if wait_for_daemon(&sock, Duration::from_millis(500)) {
            result = attempt();
        }
    }
    match result {
        Ok(Some(())) => Some(()),
        Ok(None) | Err(_) => None,
    }
}

fn main() -> ExitCode {
    if run().is_some() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
