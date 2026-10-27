//! What your own data adds to the menu, through the daemon's request handler: snippets, the (opt-in) record of
//! commands, warnings, values used before, variables and corrected typos.
//! One test: it points `GOH_CONFIG` at a private directory, which is process-wide.

use std::sync::Arc;

use goh_core::protocol::{Candidate, Kind};
use goh_core::Engine;
use goh_daemon::runs::{self, RunsCache};
use goh_daemon::server::Server;
use goh_daemon::wire::{buffer_hash, Reply, Request};

const SESSION: &str = "local-data-session";

struct Daemon {
    server: Server,
    generation: u64,
}

impl Daemon {
    fn ask(&mut self, buffer: &str, live: bool) -> Vec<Candidate> {
        self.generation += 1;
        let reply = self.server.handle(Request::Complete {
            v: 1, session_id: SESSION.into(), generation: self.generation, shell: "zsh".into(), cwd: "/work".into(),
            buffer: buffer.into(), cursor_byte: buffer.len(), editor_state: "editing".into(), live,
        });
        match reply {
            Some(Reply::Results { suggestions, .. }) => suggestions,
            other => panic!("unexpected reply {other:?}"),
        }
    }
}

#[test]
fn local_data_reaches_the_menu() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("GOH_CONFIG", dir.path().join("config.json"));
    std::env::set_var("HOME", dir.path()); // no kube config here: nothing says which cluster
    std::fs::write(dir.path().join("snippets.json"), r#"[{"name":"gcm","template":"git commit -m \"{{cursor}}\"","description":"Commit with a message"}]"#).unwrap();

    let file = dir.path().join("runs");
    let now = goh_daemon::visits::now();
    // Twice `git add .` then the same commit line; `kubectl -n staging`; one secret that must never be suggested.
    for (ago, cwd, line) in [(900, "/work", "git add ."), (890, "/work", "git commit -m ship"), (400, "/work", "git add ."), (390, "/work", "git commit -m ship"),
        (200, "/work", "kubectl -n staging get pods"), (100, "/work", "kubectl -n prod get pods"), (5, "/work", "git add .")] {
        runs::record(&file, cwd, line, now - ago).unwrap();
    }
    runs::record(&file, "/work", "git push --force https://me:pw@host/r", now).unwrap();
    assert!(!std::fs::read_to_string(&file).unwrap().contains("pw@host"));

    let engine = Engine::new(goh_core::bundled_specs());
    let mut d = Daemon { server: Server::with_runs(engine, Arc::new(RunsCache::forced(file.clone(), true))), generation: 0 };

    // Snippets lead, and accepting one puts the cursor at its marker.
    let found = d.ask("gc", true);
    assert_eq!((found[0].id.as_str(), found[0].insert_text.as_str(), found[0].cursor_offset), ("snippet:gcm", "git commit -m \"\"", Some(15)));
    let Some(Reply::Edit { cursor_byte, insert_text, .. }) = d.server.handle(Request::Accept {
        v: 1, session_id: SESSION.into(), generation: d.generation, suggestion_id: "snippet:gcm".into(), buffer_hash: buffer_hash("gc"),
    }) else { panic!("accept") };
    assert_eq!((cursor_byte, insert_text.as_str()), (15, "git commit -m \"\""));

    // A started line is finished from the record, after what the spec offers.
    let found = d.ask("git commit -m s", true);
    let history: Vec<_> = found.iter().filter(|c| c.kind == Kind::History).collect();
    assert_eq!(history.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["git commit -m ship"]);
    assert_eq!(history[0].description.as_deref(), Some("history · 2× · this folder"));
    assert_eq!((history[0].replace_start_byte, history[0].replace_end_byte), (0, 15));
    // The line with a password was never recorded, so nothing offers it.
    assert!(d.ask("git push --force", false).iter().all(|c| !c.label.contains("pw@host")));

    // The command that usually follows the last one run here, at an empty prompt.
    let found = d.ask("", true);
    assert_eq!(found.iter().map(|c| (c.label.as_str(), c.kind)).collect::<Vec<_>>(), [("git commit -m ship", Kind::History)]);
    assert_eq!(found[0].description.as_deref(), Some("usually next here · 2×"));

    // Values given to an option before come first.
    let found = d.ask("kubectl -n ", false);
    assert_eq!(found.iter().take(2).map(|c| (c.label.as_str(), c.description.as_deref())).collect::<Vec<_>>(), [("prod", Some("used before")), ("staging", Some("used before"))]);

    // Dangerous options are flagged.
    let found = d.ask("git push --", false);
    let force = found.iter().find(|c| c.label == "--force").expect("--force is offered");
    assert!(force.description.as_deref().is_some_and(|d| d.starts_with("⚠ overwrites the remote branch")), "{:?}", force.description);

    // Variables of the shell, and a slip in a subcommand.
    assert_eq!(d.server.handle(Request::Context { v: 1, session_id: SESSION.into(), aliases: vec![], functions: vec![], builtins: vec![], path: String::new(), env: vec!["HOME".into(), "HISTSIZE".into()] }).map(|r| matches!(r, Reply::Ack { .. })), Some(true));
    assert_eq!(d.ask("echo $HOM", false).iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["HOME"]);
    assert_eq!(d.ask("git chekcout", false).first().map(|c| c.label.as_str()), Some("checkout"));

    // With recording off none of the record is used.
    let mut off = Daemon { server: Server::with_runs(Engine::new(goh_core::bundled_specs()), Arc::new(RunsCache::forced(file, false))), generation: 0 };
    assert!(off.ask("git commit -m s", true).iter().all(|c| c.kind != Kind::History));
    assert!(off.ask("", true).is_empty());
    assert!(off.ask("kubectl -n ", false).iter().all(|c| c.description.as_deref() != Some("used before")));
}
