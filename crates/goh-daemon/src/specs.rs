//! Which completion specs the daemon serves: yours, then the ones bundled in the binary.

use std::path::{Path, PathBuf};

use goh_core::{bundled_specs, spec::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Bundled,
    User(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub spec: Command,
    pub source: Source,
    /// For a spec of yours that replaces a bundled one: how many subcommands the bundled one had.
    pub replaces_bundled: Option<usize>,
}

/// `$GOH_SPEC_DIR`, else `~/.config/goh/specs`.
pub fn default_dir() -> PathBuf {
    std::env::var_os("GOH_SPEC_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/goh/specs")))
        .unwrap_or_default()
}

/// Your specs (`*.json`, sorted by file name; the first for a command wins) followed by every bundled
/// spec for a command you do not define. Returns what was loaded plus a warning for each unusable file.
pub fn load(user_dir: &Path) -> (Vec<Loaded>, Vec<String>) {
    let mut out: Vec<Loaded> = Vec::new();
    let mut warnings = Vec::new();
    let bundled = bundled_specs();
    if let Ok(read) = std::fs::read_dir(user_dir) {
        let mut files: Vec<_> = read.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for path in files {
            let Some(spec) = std::fs::read_to_string(&path).ok().and_then(|s| Command::from_json(&s).ok()) else {
                warnings.push(format!("skipping invalid spec {}", path.display()));
                continue;
            };
            if out.iter().any(|l| l.spec.name == spec.name) {
                warnings.push(format!("{} defines `{}` again; the first file wins", path.display(), spec.name));
                continue;
            }
            let replaces_bundled = bundled.iter().find(|b| b.name == spec.name).map(|b| b.subcommands.len());
            out.push(Loaded { spec, source: Source::User(path), replaces_bundled });
        }
    }
    for b in bundled {
        if !out.iter().any(|l| l.spec.name == b.name) {
            out.push(Loaded { spec: b, source: Source::Bundled, replaces_bundled: None });
        }
    }
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (n, c) in files {
            std::fs::write(d.path().join(n), c).unwrap();
        }
        d
    }

    const MINI_GIT: &str = r#"{"schemaVersion":1,"name":"git","subcommands":[{"name":"checkout"}]}"#;
    const TOOL: &str = r#"{"schemaVersion":1,"name":"tool","subcommands":[{"name":"go"}]}"#;

    #[test]
    fn with_no_user_specs_the_bundled_ones_serve_the_common_commands() {
        let d = tempfile::tempdir().unwrap();
        let (loaded, warnings) = load(d.path());
        let names: Vec<_> = loaded.iter().map(|l| l.spec.name.as_str()).collect();
        assert_eq!(names.len(), bundled_specs().len());
        for name in ["git", "docker", "npm", "pnpm", "brew", "uv", "cargo", "ssh"] {
            assert!(names.contains(&name), "{name} is bundled");
        }
        assert!(loaded.iter().all(|l| l.source == Source::Bundled) && warnings.is_empty());
        let (missing, _) = load(Path::new("/nonexistent/specs"));
        assert_eq!(missing.len(), names.len(), "a missing directory is not an error");
    }

    #[test]
    fn your_spec_replaces_the_bundled_one_and_adds_new_commands() {
        let d = dir_with(&[("git.json", MINI_GIT), ("tool.json", TOOL)]);
        let (loaded, _) = load(d.path());
        let git = loaded.iter().find(|l| l.spec.name == "git").unwrap();
        assert_eq!((git.spec.subcommands.len(), matches!(git.source, Source::User(_))), (1, true));
        let bundled_git = goh_core::bundled_specs().into_iter().find(|b| b.name == "git").unwrap().subcommands.len();
        assert_eq!(git.replaces_bundled, Some(bundled_git), "it reports the size of the bundled git it replaces");
        assert!(loaded.iter().any(|l| l.spec.name == "tool"));
        assert_eq!(loaded.iter().filter(|l| l.spec.name == "git").count(), 1, "never two specs for one command");
        assert!(loaded.iter().any(|l| l.spec.name == "docker" && l.source == Source::Bundled), "the rest stay bundled");
    }

    #[test]
    fn unusable_and_duplicate_files_are_reported_not_fatal() {
        let d = dir_with(&[("a.json", MINI_GIT), ("b.json", MINI_GIT), ("bad.json", "{not json"), ("notes.txt", "ignored")]);
        let (loaded, warnings) = load(d.path());
        assert!(loaded.iter().any(|l| l.spec.name == "git"));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("bad.json")) && warnings.iter().any(|w| w.contains("first file wins")));
    }
}
