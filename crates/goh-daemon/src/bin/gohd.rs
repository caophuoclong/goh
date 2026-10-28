//! `gohd [--socket PATH] [--spec-dir DIR]` — one daemon per user, started on demand. Serves the specs in
//! the spec directory plus the ones bundled in the binary (yours win for the same command).

use std::process::ExitCode;
use std::sync::Arc;

use goh_core::Engine;
use goh_daemon::{log, paths, server::Server, specs};

fn main() -> ExitCode {
    let mut socket = paths::socket_path();
    let mut spec_dir = specs::default_dir();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match (flag.as_str(), args.next()) {
            ("--socket", Some(v)) => socket = v.into(),
            ("--spec-dir", Some(v)) => spec_dir = v.into(),
            _ => {
                eprintln!("usage: gohd [--socket PATH] [--spec-dir DIR]");
                return ExitCode::FAILURE;
            }
        }
    }

    match paths::bind(&socket) {
        Ok(Some(listener)) => {
            log::init();
            let (loaded, warnings) = specs::load(&spec_dir);
            log::line(&format!("gohd {} (pid {}) started with {} commands", env!("CARGO_PKG_VERSION"), std::process::id(), loaded.len()));
            for w in warnings {
                eprintln!("gohd: {w}");
                log::line(&format!("spec warning: {w}"));
            }
            Arc::new(Server::new(Engine::new(loaded.into_iter().map(|l| l.spec).collect()))).serve(listener);
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS, // already running
        Err(e) => {
            eprintln!("gohd: {}: {e}", socket.display());
            log::init();
            log::line(&format!("could not bind the socket: {e}"));
            ExitCode::FAILURE
        }
    }
}
