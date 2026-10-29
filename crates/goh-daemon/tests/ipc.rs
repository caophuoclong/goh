mod common;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;

use goh_core::{spec::Command, Engine};
use goh_daemon::wire::{buffer_hash, read_message, write_frame, Reply, Request};
use goh_daemon::{client, paths};

const GIT: &str = r#"{"schemaVersion":1,"name":"git","subcommands":[{"name":"checkout"},{"name":"cherry-pick"}]}"#;
const TOOL: &str = r#"{"schemaVersion":1,"name":"tool","subcommands":[{"name":"shell"},{"name":"stash"}]}"#;
const SESSION: &str = "session-0001";

fn start() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sock_dir = dir.path().join("run");
    let sock = sock_dir.join("gohd.sock");
    // bind() creates the directory itself with mode 0700.
    let listener = paths::bind(&sock).unwrap().unwrap();
    let engine = Engine::new(vec![Command::from_json(GIT).unwrap(), Command::from_json(TOOL).unwrap()]);
    let server = common::isolated_server(engine, dir.path());
    std::thread::spawn(move || Arc::new(server).serve(listener));
    (dir, sock)
}

fn complete(generation: u64, buffer: &str) -> Request {
    Request::Complete {
        v: 1,
        session_id: SESSION.into(),
        generation,
        shell: "zsh".into(),
        cwd: "/".into(),
        buffer: buffer.into(),
        cursor_byte: buffer.len(),
        editor_state: "editing".into(),
        live: false,
    }
}

fn accept(generation: u64, id: &str, buffer: &str) -> Request {
    Request::Accept {
        v: 1,
        session_id: SESSION.into(),
        generation,
        suggestion_id: id.into(),
        buffer_hash: buffer_hash(buffer),
    }
}

fn ask(s: &mut UnixStream, req: &Request) -> Reply {
    write_frame(s, req).unwrap();
    read_message(s).unwrap().unwrap()
}

#[test]
fn complete_then_accept_yields_edit() {
    let (_d, sock) = start();
    let mut s = UnixStream::connect(&sock).unwrap();
    let Reply::Results { suggestions, .. } = ask(&mut s, &complete(10, "git ch")) else { panic!() };
    assert_eq!(suggestions[0].id, "git:checkout");
    match ask(&mut s, &accept(10, "git:checkout", "git ch")) {
        Reply::Edit { replace_start_byte, replace_end_byte, insert_text, cursor_byte, .. } => {
            assert_eq!((replace_start_byte, replace_end_byte, insert_text.as_str(), cursor_byte), (4, 6, "checkout ", 13));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn accept_is_stale_when_buffer_or_generation_changed() {
    let (_d, sock) = start();
    let mut s = UnixStream::connect(&sock).unwrap();
    ask(&mut s, &complete(10, "git ch"));
    assert!(matches!(ask(&mut s, &accept(10, "git:checkout", "git che")), Reply::Stale { .. }));
    assert!(matches!(ask(&mut s, &accept(9, "git:checkout", "git ch")), Reply::Stale { .. }));
    assert!(matches!(ask(&mut s, &accept(10, "git:nope", "git ch")), Reply::Stale { .. }));
    // An older complete never overwrites newer state.
    ask(&mut s, &complete(20, "git ch"));
    assert!(matches!(ask(&mut s, &complete(15, "git ch")), Reply::Stale { .. }));
    assert!(matches!(ask(&mut s, &accept(20, "git:checkout", "git ch")), Reply::Edit { .. }));
}

#[test]
fn malformed_or_oversized_frames_close_the_connection() {
    let (_d, sock) = start();

    let mut s = UnixStream::connect(&sock).unwrap();
    s.write_all(&(20_000u32).to_be_bytes()).unwrap();
    let mut buf = [0u8; 1];
    assert_eq!(s.read(&mut buf).unwrap(), 0);

    let mut s = UnixStream::connect(&sock).unwrap();
    s.write_all(&5u32.to_be_bytes()).unwrap();
    s.write_all(b"{nope").unwrap();
    assert_eq!(s.read(&mut buf).unwrap(), 0);

    // Bad session id: dropped without a reply.
    let mut s = UnixStream::connect(&sock).unwrap();
    let Request::Complete { v, generation, shell, cwd, buffer, cursor_byte, editor_state, .. } = complete(1, "git") else { panic!() };
    let bad = Request::Complete { v, session_id: "x".into(), generation, shell, cwd, buffer, cursor_byte, editor_state, live: false };
    write_frame(&mut s, &bad).unwrap();
    assert_eq!(s.read(&mut buf).unwrap(), 0);

    // The daemon is still healthy afterwards.
    let mut s = UnixStream::connect(&sock).unwrap();
    assert!(matches!(ask(&mut s, &complete(1, "git ")), Reply::Results { .. }));
}

#[test]
fn client_menu_returns_validated_items() {
    let (_d, sock) = start();
    let items = client::menu(&sock, SESSION, "/", "git ch", 6, 8, false).unwrap();
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["checkout", "cherry-pick"]);
    let e = &items[0].edit;
    assert_eq!((e.start, e.end, e.cursor, e.insert.as_str()), (4, 6, 13, "checkout "));
    assert_eq!(items[0].kind, "subcommand");
    assert_eq!(client::menu(&sock, SESSION, "/", "git ch", 6, 1, false).unwrap().len(), 1);
    assert!(client::menu(&sock, SESSION, "/", "git zzz", 7, 8, false).unwrap().is_empty());
}

#[test]
fn client_fails_open_without_daemon() {
    let dir = tempfile::tempdir().unwrap();
    assert!(client::menu(&dir.path().join("none.sock"), SESSION, "/", "git ch", 6, 8, false).is_err());
}

#[test]
fn client_fails_open_when_daemon_dies() {
    let (_d, sock) = start();
    // A "daemon" that accepts and hangs up without answering.
    let dir = tempfile::tempdir().unwrap();
    let dead = dir.path().join("dead.sock");
    let l = std::os::unix::net::UnixListener::bind(&dead).unwrap();
    std::thread::spawn(move || drop(l.accept()));
    assert!(client::menu(&dead, SESSION, "/", "git ch", 6, 8, false).is_err());
    // ...and the real one is unaffected.
    assert!(!client::menu(&sock, SESSION, "/", "git ch", 6, 8, false).unwrap().is_empty());
}

#[test]
fn socket_is_private_and_stale_socket_is_replaced() {
    let (d, sock) = start();
    assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(sock.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    // A live daemon is not displaced.
    assert!(paths::bind(&sock).unwrap().is_none());

    // A dead socket file is replaced.
    let stale = d.path().join("run/stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    assert!(paths::bind(&stale).unwrap().is_some());

    // A group-readable directory is refused.
    let loose = d.path().join("loose");
    std::fs::create_dir(&loose).unwrap();
    std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(paths::bind(&loose.join("s.sock")).is_err());
}

#[test]
fn live_requests_get_suggestions_only_for_a_recognised_command_in_progress() {
    let (_d, sock) = start();
    let labels = |buf: &str, live| -> Vec<String> {
        client::menu(&sock, SESSION, "/", buf, buf.len(), 8, live).unwrap().into_iter().map(|i| i.label).collect()
    };
    assert_eq!(labels("git c", true), ["checkout", "cherry-pick"]);
    assert_eq!(labels("git", true), ["checkout", "cherry-pick"]);
    let bare = client::menu(&sock, SESSION, "/", "git", 3, 8, true).unwrap();
    assert_eq!((bare[0].edit.start, bare[0].edit.end, bare[0].edit.insert.as_str()), (3, 3, " checkout "));
    assert!(labels("git foo c", true).is_empty(), "foo is not a git subcommand");
    assert!(labels("zzz c", true).is_empty(), "no spec for zzz");
    // An explicit request (Tab) is not filtered by validity: a command with no spec still gets file names.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "").unwrap();
    let cwd = dir.path().to_str().unwrap();
    let files = |live| -> Vec<String> {
        client::menu(&sock, SESSION, cwd, "cat no", 6, 8, live).unwrap().into_iter().map(|i| i.label).collect()
    };
    assert_eq!(files(false), ["note.txt"]);
    assert!(files(true).is_empty());
}

#[test]
fn live_requests_drop_fuzzy_matches_when_a_prefix_matches() {
    let (_d, sock) = start();
    let labels = |live| -> Vec<String> {
        client::menu(&sock, SESSION, "/", "tool sh", 7, 8, live).unwrap().into_iter().map(|i| i.label).collect()
    };
    assert_eq!(labels(false), ["shell", "stash"], "asked for: stash matches s..h in order");
    assert_eq!(labels(true), ["shell"], "unasked: only the prefix match");
}

#[test]
fn an_older_shell_context_arriving_late_does_not_replace_a_newer_one() {
    // At a cold start two sends both wait for the daemon, and the older one can land last.
    let (_d, sock) = start();
    let send = |aliases: Vec<[String; 2]>, seq| client::context(&sock, SESSION, aliases, vec![], vec![], String::new(), vec![], String::new(), seq).unwrap();
    let labels = || -> Vec<String> { client::menu(&sock, SESSION, "/", "gg ch", 5, 8, false).unwrap().into_iter().map(|i| i.label).collect() };
    send(vec![["gg".into(), "git".into()]], 2);
    send(vec![], 1);
    assert_eq!(labels(), ["checkout", "cherry-pick"], "the alias from the newer context stays");
    send(vec![], 3);
    assert!(labels().is_empty(), "a newer context still replaces it");
}
