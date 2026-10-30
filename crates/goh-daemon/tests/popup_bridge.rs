//! `goh popup-bridge`: the remote end of `goh ssh` relays frames and answers, and answers inline without a relay.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

fn bridge(sock: &str, input: &[u8]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_goh"))
        .args(["popup-bridge", sock])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn relays_frames_and_answers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let popup = std::thread::spawn(move || {
        let (conn, _) = listener.accept().unwrap();
        let mut w = conn.try_clone().unwrap();
        for line in BufReader::new(&conn).lines().map_while(Result::ok) {
            writeln!(w, "got {line}").unwrap();
        }
    });
    let out = bridge(path.to_str().unwrap(), b"{\"type\":\"show\"}\n");
    popup.join().unwrap();
    assert_eq!(out, "got {\"type\":\"show\"}\n");
}

#[test]
fn without_a_relay_every_menu_is_inline() {
    let dir = tempfile::tempdir().unwrap();
    let out = bridge(dir.path().join("gone.sock").to_str().unwrap(), b"{\"type\":\"show\"}\n{\"type\":\"hide\"}\n");
    assert_eq!(out.lines().count(), 1);
    assert!(out.contains(r#""display":"inline""#), "{out}");
}
