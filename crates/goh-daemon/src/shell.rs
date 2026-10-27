//! Turns what an adapter reports about its shell into the engine's [`ShellContext`]: the executables on its
//! `PATH` are found here (one directory listing each, cached), the rest comes from the shell itself.

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use goh_core::shellctx::{NameKind, ShellContext};

const MAX_EXECUTABLES: usize = 20_000;
const TTL: Duration = Duration::from_secs(60);

const MAX_ENV: usize = 2000;

fn is_variable(name: &str) -> bool {
    name.len() <= 128 && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn usable(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.starts_with('-') && !name.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Executable files in the directories of a `PATH` string, first directory first, no duplicates.
pub fn executables(path: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in path.split(':').filter(|d| d.starts_with('/')) {
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        for e in read.flatten() {
            let Ok(name) = e.file_name().into_string() else { continue };
            // metadata() follows symlinks, as most of the entries in Homebrew's bin are.
            let runnable = std::fs::metadata(e.path()).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if runnable && usable(&name) && seen.insert(name.clone()) {
                out.push(name);
                if out.len() >= MAX_EXECUTABLES {
                    return out;
                }
            }
        }
    }
    out
}

#[derive(Default)]
pub struct Shells {
    path_cache: Mutex<HashMap<String, (Instant, Vec<String>)>>,
}

impl Shells {
    pub fn context(&self, aliases: &[[String; 2]], functions: &[String], builtins: &[String], path: &str, env: &[String]) -> ShellContext {
        let mut ctx = ShellContext { env: env.iter().filter(|n| is_variable(n)).take(MAX_ENV).cloned().collect(), ..Default::default() };
        let mut seen = HashSet::new();
        for [name, value] in aliases.iter().filter(|[n, v]| usable(n) && !v.is_empty()) {
            ctx.aliases.insert(name.clone(), value.clone());
            if seen.insert(name.clone()) {
                ctx.names.push((name.clone(), NameKind::Alias));
            }
        }
        for (list, kind) in [(functions, NameKind::Function), (builtins, NameKind::Builtin)] {
            for n in list.iter().filter(|n| usable(n)) {
                if seen.insert(n.clone()) {
                    ctx.names.push((n.clone(), kind));
                }
            }
        }
        for n in self.path_executables(path) {
            if seen.insert(n.clone()) {
                ctx.names.push((n, NameKind::Executable));
            }
        }
        ctx
    }

    fn path_executables(&self, path: &str) -> Vec<String> {
        let Ok(mut cache) = self.path_cache.lock() else { return vec![] };
        if let Some((at, names)) = cache.get(path).filter(|(at, _)| at.elapsed() < TTL) {
            let _ = at;
            return names.clone();
        }
        let names = executables(path);
        cache.clear(); // one PATH per shell is the norm; never grow
        cache.insert(path.to_string(), (Instant::now(), names.clone()));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executables_are_runnable_files_in_path_order_without_duplicates() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let make = |d: &std::path::Path, n: &str, mode| {
            std::fs::write(d.join(n), "").unwrap();
            std::fs::set_permissions(d.join(n), std::fs::Permissions::from_mode(mode)).unwrap();
        };
        make(a.path(), "tool", 0o755);
        make(a.path(), "notes.txt", 0o644);
        make(b.path(), "tool", 0o755);
        make(b.path(), "other", 0o755);
        std::fs::create_dir(a.path().join("subdir")).unwrap();
        let path = format!("{}:{}:relative/dir", a.path().display(), b.path().display());
        let mut found = executables(&path);
        found.sort();
        assert_eq!(found, ["other", "tool"]);
    }

    #[test]
    fn aliases_win_over_functions_and_commands_of_the_same_name() {
        let shells = Shells::default();
        let ctx = shells.context(&[["g".into(), "git".into()]], &["g".into(), "mkcd".into()], &["cd".into()], "", &["HOME".into(), "bad name".into(), "9x".into()]);
        assert_eq!(ctx.names, [("g".to_string(), NameKind::Alias), ("mkcd".to_string(), NameKind::Function), ("cd".to_string(), NameKind::Builtin)]);
        assert_eq!(ctx.aliases["g"], "git");
        assert_eq!(ctx.env, ["HOME"], "only valid variable names are kept");
    }
}
