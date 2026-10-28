//! `goh`: the command line for goh. Utility commands (run `goh help`):
//!   goh start | stop | restart | reload | status | logs | doctor [--fix] [--test] [--report] | init SHELL | specs | theme | version | help
//! The rest is what the shell adapters call.
//!
//! Adapter helper.
//!   goh menu     --session S --cwd D --cursor-byte N --buffer B [--limit N] [--live]
//!       --live: as-you-type; no output unless the line is a recognised command in progress
//!       -> one line per candidate: start US end US cursor US kind US label US insert US description US ranges US value US used
//!          (ranges = `s-e,s-e` byte ranges of the label that matched; may be empty)
//!          (value = `1` for an option that takes a value; used = `1` when picked before; both empty otherwise)
//!          (adapters read fields by position: new ones go at the end)
//!          (US = 0x1f; offsets are UTF-8 bytes)
//!   goh complete --session S --cwd D --cursor-byte N --buffer B   -> results JSON
//!   goh specs
//!       -> specs on disk for the next daemon start (the running daemon may differ)
//!          one line per command: name TAB source TAB subcommands TAB replaced-bundled-count
//!          (source: `bundled` or `yours:PATH`)
//!          stderr explains that these are not the running daemon's loaded specs
//!   goh theme [--probe] [--json]
//!       -> resolved design tokens as `key value` lines (or JSON); --probe asks the terminal for its colours
//!   goh doctor [--fix] [--test] [--report]
//!       -> checks binaries, daemon (and its version), adapter, specs, terminal and the macOS Accessibility permission;
//!          --fix starts or restarts the daemon and opens System Settings > Accessibility; --test times a real
//!          completion; --report prints a paste-able summary; exit 1 if something is broken
//! Exit 1 with no output on any failure; if the daemon is not running, start it for next time.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::process::ExitCode;

use goh_daemon::{client, control, paths, probe, wire::Reply};
use goh_theme::{parse_colorfgbg, resolve, Mode, Terminal, ThemeConfig};

/// Theme section of `$GOH_CONFIG` or `~/.config/goh/config.json`; anything unusable means `auto`.
fn theme_mode() -> Mode {
    let cfg = goh_theme::read_config();
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

/// Specs on disk for the next daemon start, not the running daemon's startup snapshot.
fn list_specs(args: &[String]) -> Option<()> {
    use goh_daemon::specs::{self, Source};
    if args.iter().any(|a| a == "--missing") {
        return missing_specs();
    }
    let (loaded, _) = specs::load(&specs::default_dir());
    eprintln!("goh: specs for the next daemon start; running daemon may differ. Restart gohd after changes.");
    for l in loaded {
        let source = match &l.source {
            Source::Bundled => "bundled".to_string(),
            Source::User(p) => format!("yours:{}", p.display()),
        };
        println!("{}\t{}\t{}\t{}", l.spec.name, source, l.spec.subcommands.len(), l.replaces_bundled.map(|n| n.to_string()).unwrap_or_default());
    }
    Some(())
}

/// Programs you have run that are on your PATH but have no spec, most-run first: what to write or import next.
/// Reads the opt-in record of commands; without it there is nothing to count.
fn missing_specs() -> Option<()> {
    use goh_daemon::{runs, specs};
    if !runs::enabled() {
        eprintln!("goh: commands are not recorded, so there is nothing to count. Turn on `privacy.recordCommands` in `goh config` (it stays on this machine).");
        return None;
    }
    let (loaded, _) = specs::load(&specs::default_dir());
    let known: std::collections::HashSet<&str> = loaded.iter().map(|l| l.spec.name.as_str()).collect();
    let path = std::env::var("PATH").unwrap_or_default();
    let record = std::fs::read_to_string(runs::path()).unwrap_or_default();
    let missing: Vec<_> = runs::programs(&record)
        .into_iter()
        .filter(|(name, _)| !known.contains(name.as_str()) && goh_daemon::setup::on_path(&path, name))
        .collect();
    if missing.is_empty() {
        println!("every program you ran has a spec (or is not on your PATH)");
    }
    for (name, n) in missing.iter().take(30) {
        println!("{n:>5}  {name}\t`goh spec gen {name}` writes one from its --help");
    }
    Some(())
}

/// `goh logs [-n N] [--path]`: the daemon's own log, which holds no command text.
fn logs(rest: &[String]) -> bool {
    let file = goh_daemon::log::path();
    if rest.iter().any(|a| a == "--path") {
        println!("{}", file.display());
        return true;
    }
    let n = rest.iter().position(|a| a == "-n").and_then(|i| rest.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(50);
    match goh_daemon::log::tail(&file, n) {
        Some(text) if !text.is_empty() => println!("{text}"),
        _ => println!("no log yet ({}); the daemon writes one when it starts", file.display()),
    }
    true
}

fn doctor(args: &[String]) -> Option<()> {
    use goh_daemon::doctor::{environment, evaluate, gather, label, report, round_trip, Level};
    let (mut fix, mut test, mut want_report) = (false, false, false);
    for a in args {
        match a.as_str() {
            "--fix" => fix = true,
            "--test" => test = true,
            "--report" => want_report = true,
            other => {
                eprintln!("goh: unknown option `{other}`; usage: goh doctor [--fix] [--test] [--report]");
                return None;
            }
        }
    }
    let mut inputs = gather();
    if test {
        inputs.round_trip = Some(if inputs.daemon_running { round_trip(&paths::socket_path()) } else { goh_daemon::doctor::RoundTrip::Failed("gohd is not running".into()) });
    }
    let checks = evaluate(&inputs);
    if want_report {
        let home = std::env::var("HOME").ok();
        print!("{}", report(env!("CARGO_PKG_VERSION"), &environment(), &checks, home.as_deref()));
        return (!checks.iter().any(|c| c.level == Level::Fail)).then_some(());
    }
    for c in &checks {
        let hint = if c.fix.is_some() && !fix { "  (fixable: goh doctor --fix)" } else { "" };
        println!("{}  {:<14} {}{hint}", label(c.level), c.name, c.detail);
    }
    if fix {
        for f in checks.iter().filter_map(|c| c.fix) {
            println!("fix   {}", goh_daemon::doctor::fix(f));
        }
    }
    (!checks.iter().any(|c| c.level == Level::Fail)).then_some(())
}

const USAGE: &str = "goh: spec-driven terminal completion

usage: goh <command>

  status            is the daemon running, which specs and history it uses
  start             start the daemon (it also starts on the first Tab)
  stop              stop the daemon
  restart           stop, then start the daemon
  reload            restart the daemon so it re-reads your specs (~/.config/goh/specs), and the popups so they re-read the font
  doctor [--fix] [--test] [--report]
                    check the setup; --fix starts or restarts the daemon and opens the macOS Accessibility settings;
                    --test asks the daemon to complete `git chec` and times it; --report prints the result for a bug
                    report (versions and checks, your home folder shown as ~, no command text)
  logs [-n N] [--path]   the daemon's log (starts, spec warnings, refused connections, panics; never what you typed)
  setup [--shell S] [--rc FILE] [--dry-run] [--uninstall]
                    add the adapter to your shell's startup file (zsh, bash, fish) in one marked block;
                    --uninstall removes exactly that block
  init <shell>      print the adapter for zsh, bash, fish, pwsh, nu, elvish or xonsh, e.g.
                      eval \"$(goh init zsh)\"          # in ~/.zshrc
  forget            delete what goh has learned from your choices and the commands it recorded (`~/.local/share/goh/usage`, `runs`)
  spec gen <command> [--print] [--force]   write a spec for a command that has none, read from its --help
  config            open the settings file in your editor (created on first use)
  config path|show|env   where it is, what is in effect, what the shell adapters read
  specs [--missing] list the completion specs (bundled and yours); --missing lists programs you ran that have none
                    (needs `privacy.recordCommands`)
  theme [--probe] [--json]   show the resolved menu colours
  version           print the version
  help              this text

An open shell keeps the adapter it already loaded: after changing adapters, run `exec $SHELL`.
";

/// Print without panicking when the reader goes away (`goh init zsh | head`).
fn out(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(text.as_bytes());
}

fn adapter(shell: &str) -> Option<&'static str> {
    Some(match shell {
        "zsh" => include_str!("../../../../adapters/zsh/goh.zsh"),
        "bash" => include_str!("../../../../adapters/bash/goh.bash"),
        "fish" => include_str!("../../../../adapters/fish/goh.fish"),
        "pwsh" | "powershell" => include_str!("../../../../adapters/powershell/goh.ps1"),
        "nu" | "nushell" => include_str!("../../../../adapters/nushell/goh.nu"),
        "elvish" | "elv" => include_str!("../../../../adapters/elvish/goh.elv"),
        "xonsh" => include_str!("../../../../adapters/xonsh/goh.xsh"),
        _ => return None,
    })
}

/// `goh setup`: add (or with --uninstall remove) the marked block in the shell's startup file.
fn setup(rest: &[String]) -> bool {
    use goh_daemon::setup as s;
    let fail = |m: String| {
        eprintln!("goh: {m}");
        false
    };
    let (mut shell, mut rc, mut dry, mut remove) = (None, None, false, false);
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--shell" => shell = it.next().cloned(),
            "--rc" => rc = it.next().map(PathBuf::from),
            "--dry-run" => dry = true,
            "--uninstall" => remove = true,
            other => return fail(format!("unknown option `{other}`; usage: goh setup [--shell zsh|bash|fish] [--rc FILE] [--dry-run] [--uninstall]")),
        }
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let shell = shell.or_else(|| env("GOH_SHELL")).or_else(|| env("SHELL").and_then(|v| Path::new(&v).file_name().map(|n| n.to_string_lossy().into_owned())));
    let Some(shell) = shell.filter(|sh| s::supported(sh)) else {
        return fail("could not tell your shell (zsh, bash and fish are supported); pass --shell, or add `eval \"$(goh init <shell>)\"` to its startup file yourself".into());
    };
    let home = env("HOME").map(PathBuf::from);
    let Some(rc) = rc.or_else(|| env("GOH_RC").map(PathBuf::from)).or_else(|| {
        let home = home.as_deref()?;
        s::rc_path(&shell, home, env("ZDOTDIR").map(PathBuf::from).as_deref(), env("XDG_CONFIG_HOME").map(PathBuf::from).as_deref(), cfg!(target_os = "macos"))
    }) else {
        return fail("no HOME: pass --rc FILE".into());
    };
    let text = match std::fs::read_to_string(&rc) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return fail(format!("{}: {e}", rc.display())),
    };
    let new = if remove {
        s::remove_block(&text)
    } else {
        // A folder is added to PATH only when the shell would not find `goh` already (Homebrew's is on it).
        let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let found = env("PATH").is_some_and(|p| s::on_path(&p, "goh"));
        s::add_block(&text, &s::block(&shell, if found { None } else { dir.as_deref() }))
    };
    let Some(new) = new else {
        println!("{} {}", rc.display(), if remove { "has no goh block; nothing to remove" } else { "already has the goh block" });
        return true;
    };
    if dry {
        println!("would write {}:\n{new}", rc.display());
        return true;
    }
    let backup = rc.with_file_name(format!("{}.goh-backup", rc.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())));
    let mut note = String::new();
    if rc.exists() && !backup.exists() && std::fs::copy(&rc, &backup).is_ok() {
        note = format!(" (backup: {})", backup.display());
    }
    if let Err(e) = rc.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&rc, new)) {
        return fail(format!("{}: {e}", rc.display()));
    }
    if remove {
        println!("removed the goh block from {}", rc.display());
    } else {
        println!("added the goh block to {}{note}; open a new terminal (or run `exec $SHELL`) to use it", rc.display());
    }
    true
}

fn status() -> bool {
    let sock = paths::socket_path();
    match control::running(&sock) {
        Some(pid) => println!("daemon    running (pid {pid})"),
        None => println!("daemon    not running (the first Tab starts it, or `goh start`)"),
    }
    println!("socket    {}", sock.display());
    let dir = goh_daemon::specs::default_dir();
    let (loaded, warnings) = goh_daemon::specs::load(&dir);
    println!("specs     {} commands on disk (yours: {})", loaded.len(), dir.display());
    warnings.iter().for_each(|w| println!("          {w}"));
    println!("history   {}", goh_daemon::visits::path().display());
    let (recording, file) = (goh_daemon::runs::enabled(), goh_daemon::runs::path());
    println!("commands  {} ({}; `privacy.recordCommands` in `goh config`)", if recording { "recorded" } else { "not recorded" }, file.display());
    true
}

fn start(sock: &std::path::Path) -> Result<(), String> {
    if control::running(sock).is_some() {
        return Ok(());
    }
    control::spawn().map_err(|e| format!("could not start gohd: {e}"))?;
    control::wait_until_up(sock, Duration::from_secs(2)).then_some(()).ok_or_else(|| "gohd did not come up within 2 s".to_string())
}

/// Stop the overlay popups of this user. Each shell runs its own and reads the font settings once at start, so a
/// running one would keep the old font; the adapter notices it is gone and starts a fresh one at the next menu.
/// Returns how many were running.
fn stop_popups() -> usize {
    use std::process::{Command, Stdio};
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() }.to_string();
    let running = |args: &[&str]| Command::new("pgrep").args(args).args(["-U", &uid, "-x", "goh-popup"]).stderr(Stdio::null()).output().map_or(0, |o| String::from_utf8_lossy(&o.stdout).lines().count());
    let count = running(&[]);
    if count > 0 {
        let _ = Command::new("pkill").args(["-U", &uid, "-x", "goh-popup"]).stderr(Stdio::null()).status();
    }
    count
}

fn restart(sock: &std::path::Path) -> Result<(), String> {
    control::stop(sock).map_err(|e| e.to_string())?;
    start(sock)
}

fn spec(rest: &[String]) -> bool {
    use goh_daemon::{helpgen, specs};
    let fail = |m: String| {
        eprintln!("goh: {m}");
        false
    };
    let (print, force) = (rest.iter().any(|a| a == "--print"), rest.iter().any(|a| a == "--force"));
    let [verb, name, ..] = rest else { return fail("usage: goh spec gen <command> [--print] [--force]".into()) };
    if verb != "gen" || name.starts_with('-') {
        return fail("usage: goh spec gen <command> [--print] [--force]".into());
    }
    let help = match helpgen::run_help(name) {
        Ok(h) => h,
        Err(e) => return fail(e),
    };
    let spec = helpgen::from_help(name, &help);
    let json = serde_json::to_string_pretty(&spec).unwrap_or_default() + "\n";
    if goh_core::spec::Command::from_json(&json).is_err() {
        return fail("the help text did not produce a usable spec".into());
    }
    let counts = |k: &str| spec.get(k).and_then(|v| v.as_array()).map_or(0, Vec::len);
    if print {
        out(&json);
        return true;
    }
    let file = specs::default_dir().join(format!("{name}.json"));
    if file.exists() && !force {
        return fail(format!("{} exists; use --force to replace it, or --print to look at the new one", file.display()));
    }
    if goh_core::bundled_specs().iter().any(|b| &b.name == name) && !force {
        return fail(format!("`{name}` already has a bundled spec, which is richer than one read from --help; use --force to override it"));
    }
    if let Err(e) = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&file, &json)) {
        return fail(format!("{}: {e}", file.display()));
    }
    println!("wrote {}: {} options, {} subcommands; `goh reload` to use it", file.display(), counts("options"), counts("subcommands"));
    true
}

fn config(sub: Option<&str>) -> bool {
    use goh_daemon::config;
    let fail = |m: String| {
        eprintln!("goh: {m}");
        false
    };
    match sub {
        None => {
            let path = match config::ensure() {
                Ok(p) => p,
                Err(e) => return fail(format!("{e}")),
            };
            match config::open_in_editor(&path) {
                Ok(done) => {
                    // A terminal editor has exited, so what was saved can be checked right away.
                    match (done, std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| config::validate(&t))) {
                        (true, Err(e)) => fail(format!("{}: {e}", path.display())),
                        (true, Ok(())) => {
                            println!("saved {} (open shells pick it up after `exec $SHELL`; the daemon needs nothing)", path.display());
                            true
                        }
                        (false, _) => {
                            println!("opened {} in your default editor; `goh doctor` checks it afterwards", path.display());
                            true
                        }
                    }
                }
                Err(e) => fail(format!("{e}")),
            }
        }
        Some("path") => match config::path() {
            Ok(p) => {
                println!("{}", p.display());
                true
            }
            Err(e) => fail(e.to_string()),
        },
        Some("show") => {
            let path = config::path().ok();
            match path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
                Some(text) => {
                    out(&text);
                    config::validate(&text).map_or_else(|e| fail(e), |()| true)
                }
                None => {
                    println!("no config file ({}); defaults are in effect. `goh config` creates one.", path.map_or_else(|| "no home".into(), |p| p.display().to_string()));
                    true
                }
            }
        }
        Some("env") => {
            // For the shell adapters: silent on any problem, so a broken file never breaks a prompt.
            if let Some(v) = goh_theme::read_config() {
                config::env_lines(&v).iter().for_each(|l| println!("{l}"));
            }
            true
        }
        Some(other) => fail(format!("unknown config command `{other}`; see `goh help`")),
    }
}

/// Utility commands. `None`: not one of them (the adapter commands follow).
fn utility(sub: &str, rest: &[String]) -> Option<bool> {
    let sock = paths::socket_path();
    let report = |r: Result<(), String>, ok: &str| match r {
        Ok(()) => {
            println!("{ok}");
            true
        }
        Err(e) => {
            eprintln!("goh: {e}");
            false
        }
    };
    Some(match sub {
        "help" | "--help" | "-h" => {
            out(USAGE);
            true
        }
        "version" | "--version" | "-V" => {
            println!("goh {}", env!("CARGO_PKG_VERSION"));
            true
        }
        "status" => status(),
        "logs" => logs(rest),
        "spec" => spec(rest),
        "setup" => setup(rest),
        "forget" => {
            let results = [goh_daemon::usage::forget(&goh_daemon::usage::path()), goh_daemon::runs::forget(&goh_daemon::runs::path())];
            match results.iter().find_map(|r| r.as_ref().err()) {
                Some(e) => report(Err(e.to_string()), ""),
                None if results.iter().any(|r| matches!(r, Ok(true))) => report(Ok(()), "forgot what goh learned from your choices and the commands it recorded"),
                None => report(Ok(()), "nothing to forget"),
            }
        }
        "config" => config(rest.first().map(String::as_str)),
        "start" => report(start(&sock), "gohd is running"),
        "stop" => match control::stop(&sock) {
            Ok(Some(pid)) => report(Ok(()), &format!("stopped gohd (pid {pid})")),
            Ok(None) => report(Ok(()), "gohd was not running"),
            Err(e) => report(Err(e.to_string()), ""),
        },
        "restart" => report(restart(&sock), "restarted gohd"),
        "reload" => {
            let count = goh_daemon::specs::load(&goh_daemon::specs::default_dir()).0.len();
            let result = restart(&sock);
            let popups = if result.is_ok() { stop_popups() } else { 0 };
            let note = if popups > 0 { format!("; restarted {popups} popup(s), so the font and size are re-read at the next menu") } else { String::new() };
            report(result, &format!("reloaded: gohd now serves {count} commands{note} (open shells keep their adapter; `exec $SHELL` reloads it)"))
        }
        "init" => match rest.first().map(String::as_str).and_then(adapter) {
            Some(script) => {
                out(script);
                true
            }
            None => {
                eprintln!("usage: goh init <zsh|bash|fish|pwsh|nu|elvish|xonsh>");
                false
            }
        },
        _ => return None,
    })
}

/// `goh context --session S`, lines on stdin from the adapter: `A name TAB value`, `F name`, `B name`, `E name`, `P $PATH`.
fn shell_context(args: Vec<String>) -> Option<()> {
    use std::io::BufRead;
    let session = args.iter().position(|a| a == "--session").and_then(|i| args.get(i + 1))?.clone();
    let (mut aliases, mut functions, mut builtins, mut env, mut path) = (vec![], vec![], vec![], vec![], String::new());
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let mut parts = line.splitn(3, '\t');
        match (parts.next()?, parts.next(), parts.next()) {
            ("A", Some(name), Some(value)) => aliases.push([name.to_string(), value.to_string()]),
            ("F", Some(name), _) => functions.push(name.to_string()),
            ("B", Some(name), _) => builtins.push(name.to_string()),
            ("E", Some(name), _) => env.push(name.to_string()),
            ("P", Some(p), _) => path = p.to_string(),
            _ => {}
        }
    }
    let sock = paths::socket_path();
    let send = || client::context(&sock, &session, aliases.clone(), functions.clone(), builtins.clone(), path.clone(), env.clone());
    // Sent at shell start, when the daemon may not be up yet: start it, as a menu request would.
    match send() {
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            let _ = control::spawn();
            control::wait_until_up(&sock, Duration::from_millis(500)).then(|| send().ok()).flatten()
        }
        r => r.ok(),
    }
}

fn run() -> Option<()> {
    let mut args = std::env::args().skip(1);
    let Some(sub) = args.next() else {
        out(USAGE);
        return Some(());
    };
    let rest: Vec<String> = std::env::args().skip(2).collect();
    if let Some(ok) = utility(&sub, &rest) {
        return ok.then_some(());
    }
    if !matches!(sub.as_str(), "menu" | "complete" | "theme" | "specs" | "visit" | "doctor" | "context" | "learn" | "record") {
        eprintln!("goh: unknown command `{sub}`; see `goh help`");
        return None;
    }
    if sub == "theme" {
        return theme(args.collect());
    }
    if sub == "doctor" {
        return doctor(&args.collect::<Vec<_>>());
    }
    if sub == "learn" {
        // Adapters call this when you accept a suggestion; nothing is written when learning is off.
        let flag = |name: &str| rest.iter().position(|a| a == name).and_then(|i| rest.get(i + 1)).cloned();
        let (cwd, prefix, label) = (flag("--cwd")?, flag("--prefix")?, flag("--label")?);
        return (!goh_daemon::usage::enabled() || goh_daemon::usage::record(&goh_daemon::usage::path(), &cwd, &prefix, &label, goh_daemon::visits::now()).is_ok()).then_some(());
    }
    if sub == "record" {
        // Adapters call this after a command succeeded: `goh record --cwd DIR -- COMMAND LINE`. Nothing is
        // written unless `privacy.recordCommands` is on, and `runs::accept` leaves out what looks secret.
        let dash = rest.iter().position(|a| a == "--")?;
        let cwd = rest[..dash].iter().position(|a| a == "--cwd").and_then(|i| rest.get(i + 1))?;
        let command = rest[dash + 1..].join(" ");
        return (!goh_daemon::runs::enabled() || goh_daemon::runs::record(&goh_daemon::runs::path(), cwd, &command, goh_daemon::visits::now()).is_ok()).then_some(());
    }
    if sub == "context" {
        return shell_context(args.collect());
    }
    if sub == "visit" {
        // Adapters call this on every directory change; no daemon involved.
        return goh_daemon::visits::record(&goh_daemon::visits::path(), &args.next()?, goh_daemon::visits::now()).ok();
    }
    if sub == "specs" {
        return list_specs(&rest);
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
                    let flag = |b: bool| if b { "1" } else { "" };
                    println!(
                        "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
                        e.start, e.end, e.cursor, i.kind, i.label, e.insert, i.description, ranges.join(","), flag(i.takes_value), flag(i.used)
                    );
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
        let _ = control::spawn();
        if control::wait_until_up(&sock, Duration::from_millis(500)) {
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
