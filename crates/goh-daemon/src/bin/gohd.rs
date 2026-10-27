//! `gohd [--socket PATH] [--spec-dir DIR]` — one daemon per user, started on demand.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use goh_core::{spec::Command, Engine};
use goh_daemon::{paths, server::Server};

fn load_specs(dir: &PathBuf) -> Vec<Command> {
    let Ok(read) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<_> = read.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let spec = std::fs::read_to_string(&p).ok().and_then(|s| Command::from_json(&s).ok());
            if spec.is_none() {
                eprintln!("gohd: skipping invalid spec {}", p.display());
            }
            spec
        })
        .collect()
}

fn main() -> ExitCode {
    let mut socket = paths::socket_path();
    let mut spec_dir = std::env::var_os("GOH_SPEC_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/goh/specs")))
        .unwrap_or_default();
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
            Arc::new(Server::new(Engine::new(load_specs(&spec_dir)))).serve(listener);
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS, // already running
        Err(e) => {
            eprintln!("gohd: {}: {e}", socket.display());
            ExitCode::FAILURE
        }
    }
}
