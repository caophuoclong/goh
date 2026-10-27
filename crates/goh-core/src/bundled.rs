//! Completion specs shipped inside the binary, so the common commands work with no setup. A spec the
//! user installs for the same command replaces the bundled one (see `goh-daemon`'s loader).

use crate::spec::Command;

// Cargo discovers the checked-in JSON files; no network or importer is needed to build or run goh.
include!(concat!(env!("OUT_DIR"), "/bundled_specs.rs"));

/// The bundled specs. A spec that fails to parse is skipped here; a test makes sure none does.
pub fn bundled_specs() -> Vec<Command> {
    BUNDLED.iter().filter_map(|(_, json)| Command::from_json(json).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    #[test]
    fn project_arguments_have_no_demo_suggestions_but_still_accept_values() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        for prefix in ["git checkout ", "git merge ", "git switch ", "npm run "] {
            let result = engine.complete_full(prefix, prefix.len(), cwd.path()).unwrap();
            assert!(result.valid && result.candidates.iter().all(|c| c.kind == crate::protocol::Kind::Option), "{prefix:?}: {result:?}");
            let line = format!("{prefix}project-specific-value ");
            assert!(engine.complete_full(&line, line.len(), cwd.path()).unwrap().valid, "{line:?}");
        }
        let line = "git checkout --";
        let options = engine.complete(line, line.len(), cwd.path()).unwrap();
        assert!(options.iter().any(|c| c.label == "--detach"));
    }

    #[test]
    fn every_bundled_spec_parses_and_covers_its_command() {
        let specs = bundled_specs();
        assert_eq!(specs.len(), BUNDLED.len(), "a bundled spec failed to parse");
        for &(name, _) in BUNDLED {
            let s = specs.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} is bundled"));
            assert!(!s.subcommands.is_empty() || !s.options.is_empty() || !s.args.is_empty(), "{name} is empty");
        }
    }

    #[test]
    fn imported_commands_work_through_the_engine() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        for (line, label) in [("pnpm a", "add"), ("bun r", "run"), ("brew in", "install"),
            ("uv sy", "sync"), ("cargo bu", "build"), ("kubectl ge", "get"),
            ("terraform pl", "plan"), ("gh pr cr", "create"), ("aws s", "s3"),
            ("aws s3 c", "cp"), ("gcloud run dep", "deploy"),
            ("gcloud co", "compute"), ("curl --con", "--connect-timeout"), ("ssh -", "-p")] {
            let result = engine.complete_full(line, line.len(), cwd.path()).unwrap();
            assert!(result.valid && result.candidates.iter().any(|c| c.label == label), "{line:?}: {result:?}");
        }
        std::fs::create_dir(cwd.path().join("Folder")).unwrap();
        std::fs::write(cwd.path().join("File"), "").unwrap();
        let cd = engine.complete("cd F", 4, cwd.path()).unwrap();
        assert_eq!(cd.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Folder/"]);
    }
}
