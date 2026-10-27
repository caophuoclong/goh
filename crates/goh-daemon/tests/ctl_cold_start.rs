//! `goh-ctl` and a daemon that is missing or has died.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use common::kill_socket_holder;

fn ctl(socket: &Path, specs: &Path) -> (Output, Duration) {
    let t = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_goh-ctl"))
        .args(["menu", "--session", "test-session-1", "--cwd", "/", "--cursor-byte", "5", "--buffer", "git c"])
        .env("GOH_SOCKET", socket)
        .env("GOH_SPEC_DIR", specs)
        .env("GOH_CONFIG", "/nonexistent")
        .output()
        .unwrap();
    (out, t.elapsed())
}

fn labels(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split('\x1f').nth(4)).map(str::to_string).collect()
}

#[test]
fn the_first_request_after_a_cold_start_is_answered() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    fs::write(dir.path().join("specs/git.json"), common::GIT).unwrap();
    let socket = dir.path().join("run/s");

    let (out, took) = ctl(&socket, &dir.path().join("specs"));
    assert_eq!(labels(&out), ["checkout", "cherry-pick", "commit"], "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(took < Duration::from_secs(3), "{took:?}");

    // The daemon dies and leaves its socket file behind: the next request recovers the same way.
    kill_socket_holder(&socket);
    std::thread::sleep(Duration::from_millis(200));
    assert!(socket.exists(), "a stale socket file remains");
    let (out, _) = ctl(&socket, &dir.path().join("specs"));
    assert_eq!(labels(&out).len(), 3, "recovered from a stale socket");
    kill_socket_holder(&socket);
}

#[test]
fn a_daemon_that_cannot_start_means_no_output_and_failure_within_a_bound() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let (out, took) = ctl(Path::new("/dev/null/nope/s"), dir.path());
    assert!(!out.status.success() && out.stdout.is_empty());
    assert!(took < Duration::from_secs(3), "{took:?}");
}
