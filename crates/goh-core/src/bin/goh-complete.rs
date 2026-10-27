//! Headless entry point: `goh-complete --buffer "git ch" --cursor 6 --spec git.json [--cwd .] [--generation N]`
//! Prints a `results` frame as JSON.

use std::path::PathBuf;
use std::process::ExitCode;

use goh_core::{spec::Command, Engine, Response};

fn run() -> Result<(), String> {
    let (mut buffer, mut cursor, mut cwd, mut generation) = (None, None, PathBuf::from("."), 0u64);
    let mut specs = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--buffer" => buffer = Some(value()?),
            "--cursor" => cursor = Some(value()?.parse::<usize>().map_err(|e| format!("--cursor: {e}"))?),
            "--cwd" => cwd = value()?.into(),
            "--generation" => generation = value()?.parse().map_err(|e| format!("--generation: {e}"))?,
            "--spec" => {
                let path = value()?;
                let json = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                specs.push(Command::from_json(&json).map_err(|e| format!("{path}: {e}"))?);
            }
            "--json" => {}
            other => return Err(format!("unknown flag {other}")),
        }
    }
    let buffer = buffer.ok_or("--buffer is required")?;
    let cursor = cursor.unwrap_or(buffer.len());
    let suggestions = Engine::new(specs).complete(&buffer, cursor, &cwd).map_err(|e| e.to_string())?;
    let response = Response::new("cli-session".into(), generation, suggestions);
    println!("{}", serde_json::to_string_pretty(&response).map_err(|e| e.to_string())?);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("goh-complete: {e}");
            ExitCode::FAILURE
        }
    }
}
