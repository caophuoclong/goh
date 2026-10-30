//! `goh ssh`: ssh to a shell whose menus are drawn by the popup here (relay.rs carries them). The zsh adapter runs
//! `ssh` through it unless `popup.overSsh` is off.
//!
//! One connection serves everything (a control master, so you sign in once): it looks at the host, and when the
//! host's login shell is zsh and it has no goh, offers to copy this version there, into
//! `~/.local/share/goh/ssh/VERSION` (the binaries for its system, from this version's release, kept in
//! `~/.cache/goh/ssh`). That copy is loaded for the session only: your startup files on the host are read as
//! usual and none of them is changed. The answer is remembered per destination in `~/.local/share/goh/ssh-hosts`.
//! A host that has its own goh keeps it; a command line (`ssh host ls`) and anything unusual is plain ssh.

use std::io::{BufRead, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::relay::{self, VAR};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where goh's release packages are (`GOH_RELEASES` replaces it, as for install.sh).
const RELEASES: &str = "https://github.com/caophuoclong/goh/releases";

/// ssh's options that take a value (OpenSSH's getopt string).
const WITH_VALUE: &str = "bceilmopBDEFIJLOPQRSwW";
/// Options of one session, rather than of the connection: they go to the session that runs over the master.
const SESSION: &str = "AaXxYytTeqv";
/// Options after which there is no shell to draw menus for.
const NO_SHELL: &str = "NfGVOW";

/// `goh ssh ARGS...`. Returns ssh's exit code.
pub fn run(args: &[String]) -> i32 {
    let env_set = |k| std::env::var_os(k).is_some();
    // No popup here to draw for it: over ssh or inside a multiplexer this terminal's caret is not ours to find.
    let local_popup = !(env_set("SSH_CONNECTION") || env_set("SSH_TTY") || env_set("TMUX") || env_set("STY"));
    let Some(parsed) = parse(args).filter(|p| local_popup && p.command.is_empty()) else { return exec_ssh(args) };
    let Some(relay) = relay::start() else { return exec_ssh(args) };
    let control = relay.dir.join("cm");
    let control_opts = |master: &str| ["-o".to_string(), format!("ControlPath={}", control.display()), "-o".into(), format!("ControlMaster={master}")];

    let master = Command::new("ssh")
        .args(control_opts("yes"))
        .args(["-o", "ControlPersist=300", "-f", "-N", "-R"])
        .arg(format!("{}:{}", relay.remote, relay.local.display()))
        .args(&parsed.master)
        .arg(&parsed.dest)
        .status();
    match master {
        Ok(s) if s.success() => {}
        Ok(s) => return s.code().unwrap_or(255),
        Err(e) => {
            eprintln!("goh: ssh: {e}");
            return 127;
        }
    }
    let over_master = |cmd: &mut Command| {
        cmd.args(control_opts("no")).args(&parsed.session);
    };

    let mut probe_cmd = Command::new("ssh");
    over_master(&mut probe_cmd);
    let probe = probe_cmd
        .args(["-T", "-o", "LogLevel=ERROR", &parsed.dest, "sh", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().map(|mut i| i.write_all(probe_script().as_bytes()));
            c.wait_with_output()
        })
        .map(|o| Probe::parse(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();

    let ours = match plan(&probe) {
        Plan::Ours => true,
        Plan::Offer(triple) => offer(&parsed.dest, &triple) && install(&parsed.dest, &triple, over_master),
        Plan::Old(version) => {
            eprintln!("goh: {} has goh {version}, which cannot send its menus here; update it to {VERSION} for the popup", parsed.dest);
            false
        }
        Plan::Plain => false,
    };

    let mut session = Command::new("ssh");
    over_master(&mut session);
    session.args(["-o", &format!("SendEnv={VAR}")]).env(VAR, &relay.remote);
    if ours {
        let home = format!(".local/share/goh/ssh/{VERSION}");
        session.args(["-t", &parsed.dest]).arg(format!(
            "exec env GOH_USER_ZDOTDIR=\"${{ZDOTDIR:-$HOME}}\" ZDOTDIR=\"$HOME/{home}/zdotdir\" PATH=\"$HOME/{home}/bin:$PATH\" zsh -l"
        ));
    } else {
        session.arg(&parsed.dest);
    }
    let code = run_ssh(&mut session);
    let _ = Command::new("ssh").args(["-o", &format!("ControlPath={}", control.display()), "-O", "exit", &parsed.dest]).stderr(Stdio::null()).status();
    code
}

fn exec_ssh(args: &[String]) -> i32 {
    let e = Command::new("ssh").args(args).exec();
    eprintln!("goh: ssh: {e}");
    127
}

/// Runs ssh in the foreground. While it does, Ctrl+C and Ctrl+\ are ssh's to handle, not this process's.
fn run_ssh(cmd: &mut Command) -> i32 {
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("goh: ssh: {e}");
            return 127;
        }
    };
    // SAFETY: setting a signal disposition to SIG_IGN has no preconditions; the child was spawned before.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(libc::SIGQUIT, libc::SIG_IGN);
    }
    child.wait().map_or(255, |s| s.code().unwrap_or(255))
}

/// ssh's command line, split as ssh reads it: options (before or after the destination), the destination, and the
/// command from the first word after it that is not an option.
#[derive(Debug, PartialEq, Default)]
struct Parsed {
    /// Every option, for the connection.
    master: Vec<String>,
    /// The ones that belong to a session.
    session: Vec<String>,
    dest: String,
    command: Vec<String>,
}

/// `None`: no destination, or an option that means there is no interactive shell.
fn parse(args: &[String]) -> Option<Parsed> {
    let mut p = Parsed::default();
    let mut it = args.iter();
    let mut dest = None;
    while let Some(arg) = it.next() {
        if arg == "--" {
            if dest.is_none() {
                dest = it.next().cloned();
            }
            p.command.extend(it.by_ref().cloned());
            break;
        }
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            if dest.is_none() {
                dest = Some(arg.clone());
                continue;
            }
            p.command.push(arg.clone());
            p.command.extend(it.by_ref().cloned());
            break;
        };
        let (mut session, mut words) = (String::new(), vec![arg.clone()]);
        for (i, c) in flags.char_indices() {
            if NO_SHELL.contains(c) {
                return None;
            }
            if WITH_VALUE.contains(c) {
                if flags[i + c.len_utf8()..].is_empty() {
                    words.push(it.next()?.clone());
                }
                if c == 'e' || c == 'o' {
                    session = "*".into(); // the whole word (and its value) goes to the session too
                }
                break;
            }
            if SESSION.contains(c) && session != "*" {
                session.push(c);
            }
        }
        if session == "*" {
            p.session.extend(words.iter().cloned());
        } else if !session.is_empty() {
            p.session.push(format!("-{session}"));
        }
        p.master.extend(words);
    }
    p.dest = dest?;
    Some(p)
}

/// What the host said about itself (`key=value` lines from [`probe_script`]).
#[derive(Debug, Default, PartialEq)]
struct Probe {
    os: String,
    arch: String,
    shell: String,
    /// Its own goh, if any, and whether that one has `popup-bridge`.
    goh: Option<(String, bool)>,
    /// This version is already copied there.
    ours: bool,
}

impl Probe {
    fn parse(text: &str) -> Probe {
        let mut p = Probe::default();
        let (mut path, mut bridge, mut version) = (String::new(), false, String::new());
        for line in text.lines() {
            match line.split_once('=') {
                Some(("os", v)) => p.os = v.into(),
                Some(("arch", v)) => p.arch = v.into(),
                Some(("shell", v)) => p.shell = v.into(),
                Some(("goh", v)) => path = v.into(),
                Some(("bridge", v)) => bridge = v == "yes",
                Some(("version", v)) => version = v.into(),
                Some(("ours", v)) => p.ours = v == "yes",
                _ => {}
            }
        }
        if !path.is_empty() {
            p.goh = Some((if version.is_empty() { "?".into() } else { version }, bridge));
        }
        p
    }
}

/// Run by `sh` on the host, from your home folder. Only looks.
fn probe_script() -> String {
    format!(
        r#"echo "os=$(uname -s)"
echo "arch=$(uname -m)"
echo "shell=${{SHELL##*/}}"
g=$(command -v goh 2>/dev/null)
for c in "$HOME/.local/bin/goh" /opt/homebrew/bin/goh /home/linuxbrew/.linuxbrew/bin/goh /usr/local/bin/goh; do
  [ -z "$g" ] && [ -x "$c" ] && g=$c
done
if [ -n "$g" ]; then
  echo "goh=$g"
  if "$g" popup-bridge </dev/null 2>&1 | grep -q unknown; then echo bridge=no; else echo bridge=yes; fi
  echo "version=$("$g" version 2>/dev/null | sed 's/^goh //')"
fi
[ -x "$HOME/.local/share/goh/ssh/{VERSION}/bin/goh" ] && echo ours=yes
exit 0
"#
    )
}

#[derive(Debug, PartialEq)]
enum Plan {
    /// This version is there: load it for the session.
    Ours,
    /// No goh, a zsh login shell, a system with a package: offer to copy this one.
    Offer(String),
    /// The host's own goh cannot relay: say so.
    Old(String),
    Plain,
}

fn plan(p: &Probe) -> Plan {
    match &p.goh {
        Some((_, true)) => Plan::Plain,
        Some((version, false)) => Plan::Old(version.clone()),
        None if p.shell != "zsh" => Plan::Plain,
        None if p.ours => Plan::Ours,
        None => triple(&p.os, &p.arch).map_or(Plan::Plain, Plan::Offer),
    }
}

/// The release target for a host's `uname -s` and `uname -m`.
fn triple(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "Linux" => "unknown-linux-gnu",
        "Darwin" => "apple-darwin",
        _ => return None,
    };
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => return None,
    };
    Some(format!("{arch}-{os}"))
}

fn hosts_file() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(|| home().join(".local/share"), PathBuf::from);
    base.join("goh/ssh-hosts")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// The remembered answer for a destination, if any.
fn remembered(text: &str, dest: &str) -> Option<bool> {
    text.lines().rev().filter_map(|l| l.split_once('\t')).find(|(d, _)| *d == dest).map(|(_, a)| a == "yes")
}

/// Ask once per destination, on the terminal; no terminal means no.
fn offer(dest: &str, triple: &str) -> bool {
    let file = hosts_file();
    if let Some(answer) = std::fs::read_to_string(&file).ok().and_then(|t| remembered(&t, dest)) {
        return answer;
    }
    let Ok(mut tty) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty") else { return false };
    let _ = write!(
        tty,
        "goh: {dest} has no goh. Copy goh {VERSION} ({triple}) to ~/.local/share/goh/ssh there, for menus as you type?\n     Nothing else on it changes; the answer is kept in {}. [y/N] ",
        file.display()
    );
    let mut line = String::new();
    if std::io::BufReader::new(&tty).read_line(&mut line).is_err() {
        return false;
    }
    let yes = matches!(line.trim(), "y" | "Y" | "yes");
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{dest}\t{}", if yes { "yes" } else { "no" });
    }
    yes
}

/// Copy this version to the host. `false` (after saying why) when it could not.
fn install(dest: &str, triple: &str, over_master: impl Fn(&mut Command)) -> bool {
    let bins = match binaries(triple) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("goh: no goh {VERSION} for {triple}: {e}");
            return false;
        }
    };
    let stage = match tempfile_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("goh: {e}");
            return false;
        }
    };
    let staged = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(stage.join("bin"))?;
        std::fs::create_dir_all(stage.join("zdotdir"))?;
        for b in ["goh", "gohd"] {
            std::fs::copy(bins.join(b), stage.join("bin").join(b))?;
        }
        for (name, text) in ZDOTDIR {
            std::fs::write(stage.join("zdotdir").join(name), text)?;
        }
        Ok(())
    })();
    let dir = format!(".local/share/goh/ssh/{VERSION}");
    let ok = staged.is_ok() && {
        eprintln!("goh: copying goh {VERSION} to {dest}");
        // macOS's tar would add its file attributes, which GNU tar on the host warns about one by one.
        let tar = Command::new("tar").env("COPYFILE_DISABLE", "1").args(["--no-xattrs", "-C"]).arg(&stage).args(["-cf", "-", "."]).stdout(Stdio::piped()).spawn();
        tar.ok().and_then(|mut tar| {
            let mut ssh = Command::new("ssh");
            over_master(&mut ssh);
            let status = ssh
                .args(["-T", dest])
                .arg(format!(r#"d={dir}; rm -rf "$d.new" && mkdir -p "$d.new" && tar -C "$d.new" -xf - && rm -rf "$d" && mv "$d.new" "$d""#))
                .stdin(tar.stdout.take()?)
                .status();
            let _ = tar.wait();
            status.ok()
        })
        .is_some_and(|s| s.success())
    };
    let _ = std::fs::remove_dir_all(&stage);
    if !ok {
        eprintln!("goh: could not copy goh to {dest}");
    }
    ok
}

/// The session's startup files on the host: each reads yours (from where `ZDOTDIR` pointed, or your home), and the
/// last one loads goh's adapter unless yours already did. `ZDOTDIR` is yours again from `.zshrc` on, so `.zlogin`,
/// `compinit`'s dump and your history stay where they always are.
const ZDOTDIR: [(&str, &str); 3] = [
    (".zshenv", "_goh_ssh=$ZDOTDIR ZDOTDIR=$GOH_USER_ZDOTDIR\n[[ -r $ZDOTDIR/.zshenv ]] && source $ZDOTDIR/.zshenv\nGOH_USER_ZDOTDIR=$ZDOTDIR ZDOTDIR=$_goh_ssh\n"),
    (".zprofile", "_goh_ssh=$ZDOTDIR ZDOTDIR=$GOH_USER_ZDOTDIR\n[[ -r $ZDOTDIR/.zprofile ]] && source $ZDOTDIR/.zprofile\nGOH_USER_ZDOTDIR=$ZDOTDIR ZDOTDIR=$_goh_ssh\n"),
    (".zshrc", "ZDOTDIR=$GOH_USER_ZDOTDIR\nunset GOH_USER_ZDOTDIR _goh_ssh\n[[ -r $ZDOTDIR/.zshrc ]] && source $ZDOTDIR/.zshrc\n(( ${+functions[goh-load]} )) || eval \"$(goh init zsh)\"\n"),
];

fn tempfile_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("goh-ssh-stage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `goh` and `gohd` of this version for `triple`, from the cache or else from the release (checked against its
/// SHA256SUMS).
fn binaries(triple: &str) -> Result<PathBuf, String> {
    let cache = std::env::var_os("XDG_CACHE_HOME").map_or_else(|| home().join(".cache"), PathBuf::from);
    let dir = cache.join("goh/ssh").join(VERSION).join(triple);
    if ["goh", "gohd"].iter().all(|b| dir.join(b).is_file()) {
        return Ok(dir);
    }
    let releases = std::env::var("GOH_RELEASES").unwrap_or_else(|_| RELEASES.into());
    let name = format!("goh-{VERSION}-{triple}");
    let work = tempfile_dir().map_err(|e| e.to_string())?;
    let result = (|| {
        let fetch = |file: &str| -> Result<PathBuf, String> {
            let out = work.join(file);
            let ok = Command::new("curl").args(["-fsSL", "-o"]).arg(&out).arg(format!("{releases}/download/v{VERSION}/{file}")).status();
            ok.is_ok_and(|s| s.success()).then_some(out).ok_or_else(|| format!("could not download {file} from {releases}"))
        };
        let archive = fetch(&format!("{name}.tar.gz"))?;
        let sums = std::fs::read_to_string(fetch("SHA256SUMS")?).map_err(|e| e.to_string())?;
        let bytes = std::fs::read(&archive).map_err(|e| e.to_string())?;
        if !checksum_matches(&sums, &format!("{name}.tar.gz"), &bytes) {
            return Err(format!("{name}.tar.gz does not match SHA256SUMS"));
        }
        let ok = Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&work).status();
        if !ok.is_ok_and(|s| s.success()) {
            return Err(format!("could not unpack {name}.tar.gz"));
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for b in ["goh", "gohd"] {
            std::fs::copy(work.join(&name).join("bin").join(b), dir.join(b)).map_err(|e| format!("{b}: {e}"))?;
        }
        Ok(dir.clone())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn checksum_matches(sums: &str, file: &str, bytes: &[u8]) -> bool {
    use sha2::{Digest, Sha256};
    let want = format!("{:x}", Sha256::digest(bytes));
    sums.lines().filter_map(|l| l.split_once("  ")).any(|(sum, name)| name.trim() == file && sum == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn options_go_where_ssh_reads_them() {
        let p = parse(&words("-p 2222 -A -vt user@host")).unwrap();
        assert_eq!(p.dest, "user@host");
        assert_eq!(p.master, words("-p 2222 -A -vt"));
        assert_eq!(p.session, words("-A -vt"));
        assert!(p.command.is_empty());
        // Options may follow the destination; the first other word starts the command.
        let p = parse(&words("host -i ~/.ssh/k -oForwardAgent=yes ls -la")).unwrap();
        assert_eq!(p.master, words("-i ~/.ssh/k -oForwardAgent=yes"));
        assert_eq!(p.session, words("-oForwardAgent=yes"));
        assert_eq!(p.command, words("ls -la"));
        assert_eq!(parse(&words("-p22 host -- echo hi")).unwrap().command, words("echo hi"));
    }

    #[test]
    fn no_destination_or_no_shell_is_plain_ssh() {
        assert_eq!(parse(&words("-V")), None);
        assert_eq!(parse(&words("-N -L 8080:localhost:80 host")), None);
        assert_eq!(parse(&words("-p")), None);
        assert_eq!(parse(&[]), None);
    }

    #[test]
    fn the_plan_follows_what_the_host_has() {
        let probe = |text: &str| plan(&Probe::parse(text));
        assert_eq!(probe("os=Linux\narch=aarch64\nshell=zsh\n"), Plan::Offer("aarch64-unknown-linux-gnu".into()));
        assert_eq!(probe("os=Linux\narch=x86_64\nshell=zsh\nours=yes\n"), Plan::Ours);
        assert_eq!(probe("os=Linux\narch=x86_64\nshell=bash\n"), Plan::Plain);
        assert_eq!(probe("os=Linux\narch=riscv64\nshell=zsh\n"), Plan::Plain);
        assert_eq!(probe("os=Linux\narch=x86_64\nshell=zsh\ngoh=/usr/bin/goh\nbridge=yes\nversion=1.11.0\n"), Plan::Plain);
        assert_eq!(probe("os=Linux\narch=x86_64\nshell=zsh\ngoh=/usr/bin/goh\nbridge=no\nversion=1.10.1\n"), Plan::Old("1.10.1".into()));
        assert_eq!(probe(""), Plan::Plain);
    }

    #[test]
    fn the_last_answer_for_a_destination_counts() {
        let text = "pi\tno\nbox\tyes\npi\tyes\n";
        assert_eq!(remembered(text, "pi"), Some(true));
        assert_eq!(remembered(text, "box"), Some(true));
        assert_eq!(remembered(text, "other"), None);
    }

    #[test]
    fn checksums_name_the_file() {
        let sums = format!("{:x}  goh-1-x.tar.gz\n", <sha2::Sha256 as sha2::Digest>::digest(b"abc"));
        assert!(checksum_matches(&sums, "goh-1-x.tar.gz", b"abc"));
        assert!(!checksum_matches(&sums, "goh-1-x.tar.gz", b"abd"));
        assert!(!checksum_matches(&sums, "goh-1-y.tar.gz", b"abc"));
    }
}
