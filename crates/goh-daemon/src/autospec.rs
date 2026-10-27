//! Writing a spec for a command that has none, the first time it is typed (`completion.autoSpecs`, off unless you
//! turn it on). Running `<command> --help` is what `goh spec gen` does on request; this does it for you, once per
//! command, in the background, so it is a setting rather than a default.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use goh_core::shellctx::{NameKind, ShellContext};
use goh_core::{spec::Command, Engine};

const TTL: Duration = Duration::from_secs(5);

/// Programs whose `--help` should never be run on a guess: they act on the machine, or on who you are.
const NEVER: [&str; 18] = [
    "reboot", "shutdown", "halt", "poweroff", "init", "telinit", "login", "logout", "su", "sudo", "doas", "passwd", "kill", "killall",
    "pkill", "dd", "fdisk", "diskutil",
];

/// `completion.autoSpecs` in the config file.
pub fn enabled() -> bool {
    goh_theme::read_config().and_then(|c| c.pointer("/completion/autoSpecs").and_then(|v| v.as_bool())).unwrap_or(false)
}

#[derive(Default)]
pub struct AutoSpecs {
    tried: Mutex<HashSet<String>>,
    on: Mutex<Option<(Instant, bool)>>,
}

impl AutoSpecs {
    fn is_on(&self) -> bool {
        let Ok(mut cached) = self.on.lock() else { return false };
        match *cached {
            Some((at, on)) if at.elapsed() < TTL => on,
            _ => {
                let on = enabled();
                *cached = Some((Instant::now(), on));
                on
            }
        }
    }

    /// If the line's first word is a program on the user's `PATH` with no spec, start writing one. Returns at once.
    pub fn consider(&self, engine: &Arc<Engine>, buffer: &str, cursor: usize, shell: &ShellContext) {
        let first_end = buffer.find(char::is_whitespace).unwrap_or(buffer.len());
        let name = &buffer[..first_end];
        if first_end == 0 || cursor <= first_end || engine.knows(name) || NEVER.contains(&name) {
            return;
        }
        if !shell.names.iter().any(|(n, k)| n == name && *k == NameKind::Executable) || !self.is_on() {
            return;
        }
        let first_time = self.tried.lock().is_ok_and(|mut tried| tried.len() < 500 && tried.insert(name.to_string()));
        if !first_time {
            return;
        }
        let (engine, name) = (Arc::clone(engine), name.to_string());
        std::thread::spawn(move || {
            let _ = generate(&engine, &name, &crate::specs::default_dir());
        });
    }
}

/// Read `<name> --help`, and when it shows options or subcommands, save the spec in `dir` and serve it now.
/// An existing file is never replaced.
pub fn generate(engine: &Engine, name: &str, dir: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let help = crate::helpgen::run_help(name)?;
    let spec = crate::helpgen::from_help(name, &help);
    let json = serde_json::to_string_pretty(&spec).map_err(|e| e.to_string())? + "\n";
    let command = Command::from_json(&json).map_err(|e| e.to_string())?;
    if command.options.is_empty() && command.subcommands.is_empty() {
        return Err("the help text showed no options or subcommands".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{name}.json"))).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    engine.add_spec(command);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_spec_is_written_once_and_served_at_once() {
        let bin = tempfile::tempdir().unwrap();
        let tool = bin.path().join("zzgentool");
        std::fs::write(&tool, "#!/bin/sh\nprintf 'Usage: zzgentool [OPTIONS]\\n\\nOptions:\\n  -v, --verbose   More output\\n      --name <NAME>   A name\\n'\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let old = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(bin.path().to_path_buf()).chain(std::env::split_paths(&old))).unwrap();
        // A test-only change of this process's PATH, restored below; no other test starts `zzgentool`.
        std::env::set_var("PATH", &path);
        let (engine, dir) = (Engine::new(vec![]), tempfile::tempdir().unwrap());
        let made = generate(&engine, "zzgentool", dir.path());
        std::env::set_var("PATH", old);
        made.unwrap();
        assert!(engine.knows("zzgentool"));
        let text = std::fs::read_to_string(dir.path().join("zzgentool.json")).unwrap();
        assert!(text.contains("--verbose") && text.contains("--name"));
        assert!(generate(&engine, "zzgentool", dir.path()).is_err(), "an existing spec is never replaced");
    }

    #[test]
    fn nothing_starts_for_known_commands_dangerous_names_or_unfinished_words() {
        let auto = AutoSpecs::default();
        let engine = Arc::new(Engine::new(vec![Command::from_json(r#"{"schemaVersion":1,"name":"known"}"#).unwrap()]));
        let shell = ShellContext { names: vec![("known".into(), NameKind::Executable), ("reboot".into(), NameKind::Executable), ("other".into(), NameKind::Executable)], ..Default::default() };
        for (line, cursor) in [("known x", 7), ("reboot x", 8), ("other", 5), ("nothere x", 9)] {
            auto.consider(&engine, line, cursor, &shell);
        }
        assert!(auto.tried.lock().unwrap().is_empty());
    }
}
