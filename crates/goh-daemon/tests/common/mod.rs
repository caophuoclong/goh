//! Shared pseudo-terminal harness for the adapter tests.
#![allow(dead_code)]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use goh_core::{spec::Command as Spec, Engine};
use goh_daemon::{paths, runs::RunsCache, server::Server};

pub const GIT: &str = r#"{"schemaVersion":1,"name":"git","subcommands":[
  {"name":"checkout","description":"Switch branches"},
  {"name":"cherry-pick","description":"Apply commits"},
  {"name":"commit","description":"Record changes","options":[{"name":"--amend","description":"Amend the previous commit"}]}],
  "options":[{"name":"-C","takesValue":true,"args":[{"name":"path"}]}]}"#;

pub fn utf8_locale() -> Option<String> {
    let out = Command::new("locale").arg("-a").output().ok()?;
    let all = String::from_utf8_lossy(&out.stdout).to_string();
    ["en_US.UTF-8", "C.UTF-8", "en_US.utf8", "C.utf8"].iter().find(|l| all.lines().any(|x| x == **l)).map(|s| s.to_string())
}

// ---- pty plumbing ----------------------------------------------------------------------------

/// Called once with the output so far when the shell sends an OSC colour query; returns the terminal's reply.
pub type Responder = Box<dyn FnMut(&[u8]) -> Option<Vec<u8>>>;

pub struct Pty {
    pub master: File,
    child: Child,
    pub responder: Option<Responder>,
    pub queried: bool,
}

impl Pty {
    pub fn spawn(mut cmd: Command) -> Pty {
        let mut ws = libc::winsize { ws_row: 24, ws_col: 60, ws_xpixel: 0, ws_ypixel: 0 };
        let (mut m, mut s) = (0, 0);
        // SAFETY: openpty fills both fds; null name/termios are allowed.
        assert_eq!(unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null_mut(), &mut ws) }, 0);
        let slave = unsafe { OwnedFd::from_raw_fd(s) };
        let master = unsafe { File::from_raw_fd(m) };
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        cmd.stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        let child = cmd.spawn().expect("spawn");
        // SAFETY: plain fcntl on our own fd.
        unsafe { libc::fcntl(m, libc::F_SETFL, libc::O_NONBLOCK) };
        Pty { master, child, responder: None, queried: false }
    }

    /// Collect output until nothing has arrived for `quiet` (or `max` has passed).
    pub fn drain(&mut self, quiet: Duration, max: Duration) -> Vec<u8> {
        let (start, mut last) = (Instant::now(), Instant::now());
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while start.elapsed() < max && last.elapsed() < quiet {
            match self.master.read(&mut buf) {
                Ok(n) if n > 0 => {
                    out.extend_from_slice(&buf[..n]);
                    last = Instant::now();
                    if !self.queried && out.windows(6).any(|w| w == b"\x1b]10;?") {
                        self.queried = true;
                        if let Some(reply) = self.responder.as_mut().and_then(|r| r(&out)) {
                            self.master.write_all(&reply).unwrap();
                        }
                    }
                }
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        out
    }

    /// Write all of `bytes`; the master is non-blocking, so wait and retry while the pty is full.
    pub fn write_all_keys(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        let start = Instant::now();
        while !rest.is_empty() {
            match self.master.write(rest) {
                Ok(n) => rest = &rest[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(start.elapsed() < Duration::from_secs(20), "the shell stopped reading input");
                    let _ = self.drain(Duration::from_millis(5), Duration::from_millis(20)); // keep the output side flowing
                }
                Err(e) => panic!("writing to the pty failed: {e}"),
            }
        }
    }

    pub fn send(&mut self, keys: &str, quiet_ms: u64) -> String {
        self.write_all_keys(keys.as_bytes());
        String::from_utf8_lossy(&self.drain(Duration::from_millis(quiet_ms), Duration::from_secs(6))).into_owned()
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A daemon for this process that ignores the developer's own data: it runs here, so it would otherwise read this
/// process's HOME. It learns nothing (`goh learn` writes under HOME) and keeps no record of commands, so neither
/// `~/.local/share/goh` nor the zsh history (`$HISTFILE`, `~/.zsh_history`) adds candidates.
pub fn isolated_server(engine: Engine, dir: &Path) -> Server {
    Server::with_runs(engine, Arc::new(RunsCache::forced(dir.join("no-runs"), false, None))).with_usage(dir.join("no-usage"))
}

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub socket: PathBuf,
    pub locale: String,
}

impl Fixture {
    /// Short paths: Unix socket paths are limited to ~104 bytes.
    pub fn new(locale: String, daemon: bool) -> Fixture {
        let dir = tempfile::Builder::new().prefix("fz").tempdir_in("/tmp").unwrap();
        fs::create_dir(dir.path().join("specs")).unwrap();
        fs::write(dir.path().join("specs/git.json"), GIT).unwrap();
        let socket = dir.path().join("run/s");
        if daemon {
            let listener = paths::bind(&socket).unwrap().unwrap();
            let server = isolated_server(Engine::new(vec![Spec::from_json(GIT).unwrap()]), dir.path());
            std::thread::spawn(move || Arc::new(server).serve(listener));
        }
        Fixture { dir, socket, locale }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// `prog args` in a clean environment pointing at this fixture's daemon, specs and goh.
    pub fn command(&self, prog: &str, args: &[&str], extra_env: &[(&str, &str)]) -> Command {
        let ctl = Path::new(env!("CARGO_BIN_EXE_goh"));
        let bin_dir = ctl.parent().unwrap().to_str().unwrap().to_string();
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .current_dir(self.dir.path())
            .env_clear()
            .env("HOME", self.dir.path())
            .env("PATH", format!("{bin_dir}:/usr/bin:/bin"))
            .env("LC_ALL", &self.locale)
            .env("TERM", "xterm-256color")
            .env("COLUMNS", "60")
            .env("GOH_SOCKET", &self.socket)
            .env("GOH_SPEC_DIR", self.path("specs"))
            .env("GOH_CONFIG", self.path("no-config.json"))
            .env("GOH_DISPLAY", "inline") // tests must not open the real popup unless they say so
            .env("GOH_LIVE", "off") // existing tests are Tab-triggered; live tests switch it on
            .env("GOH_INLINE_STYLE", "flat") // existing tests read the flat rows; box tests switch it on
            .env("GOH_CTL", ctl);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd
    }
}

/// Stop the daemon holding the fixture's socket (one started on demand by goh). Only a process named `gohd` is
/// signalled: `lsof` also lists every other process with the socket open, and a test must never signal itself.
pub fn kill_socket_holder(socket: &Path) {
    let Ok(out) = Command::new("lsof").args(["-t"]).arg(socket).output() else { return };
    for pid in String::from_utf8_lossy(&out.stdout).split_whitespace() {
        let name = Command::new("ps").args(["-o", "comm=", "-p", pid]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if pid != std::process::id().to_string() && name.rsplit('/').next() == Some("gohd") {
            let _ = Command::new("kill").arg(pid).status();
        }
    }
}
