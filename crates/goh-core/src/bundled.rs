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

    use crate::shellctx::{NameKind, ShellContext};
    use crate::spec::{Provider, Suggestion};

    fn labels(engine: &Engine, line: &str, cwd: &std::path::Path, values: &dyn Fn(Provider, &std::path::Path) -> Vec<Suggestion>, shell: &ShellContext) -> (Vec<String>, bool) {
        let c = engine.complete_in(line, line.len(), cwd, values, shell).unwrap();
        (c.candidates.into_iter().map(|c| c.label).collect(), c.valid)
    }

    fn none(_: Provider, _: &std::path::Path) -> Vec<Suggestion> {
        vec![]
    }

    #[test]
    fn wrappers_and_assignments_hand_the_rest_of_the_line_to_the_command_they_run() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        let shell = ShellContext { names: vec![("htop".into(), NameKind::Executable)], ..Default::default() };
        for line in ["sudo git ch", "time git ch", "env FOO=1 git ch", "FOO=1 BAR=2 git ch", "sudo -u root git ch", "nohup git ch",
            "watch -n 1 git ch", "xargs -0 git ch", "timeout 5s git ch", "sudo env A=1 time git ch"] {
            let (found, valid) = labels(&engine, line, cwd.path(), &none, &shell);
            assert!(valid && found.contains(&"checkout".to_string()), "{line:?}: {found:?}");
        }
        let (found, _) = labels(&engine, "sudo ht", cwd.path(), &none, &shell);
        assert!(found.contains(&"htop".to_string()), "the command word completes from what the shell can run: {found:?}");
        let (found, _) = labels(&engine, "sudo -", cwd.path(), &none, &shell);
        assert!(found.contains(&"-u".to_string()), "the wrapper's own options: {found:?}");
        let (_, valid) = labels(&engine, "sudo nosuchtool x", cwd.path(), &none, &shell);
        assert!(!valid, "an unknown command after a wrapper is not a recognised line");
        let (found, _) = labels(&engine, "man gi", cwd.path(), &none, &shell);
        assert!(found.contains(&"git".to_string()), "man takes command names: {found:?}");
        let bins = |p: Provider, _: &std::path::Path| if p == Provider::NodeBins { vec![Suggestion { name: "mytool".into(), description: None, insert_value: None }] } else { vec![] };
        let (found, _) = labels(&engine, "npx ", cwd.path(), &bins, &shell);
        assert_eq!(found.first().map(String::as_str), Some("mytool"), "the project's own programs first: {found:?}");
        let (found, _) = labels(&engine, "npx tsc --wat", cwd.path(), &none, &shell);
        assert!(found.contains(&"--watch".to_string()), "npx runs a program completed by its own spec: {found:?}");
    }

    #[test]
    fn options_given_once_are_not_offered_again_and_letters_join_into_groups() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        let shell = ShellContext::default();
        let (found, _) = labels(&engine, "git commit --amend --am", cwd.path(), &none, &shell);
        assert!(!found.contains(&"--amend".to_string()), "{found:?}");
        let (found, _) = labels(&engine, "git commit --amend --amend", cwd.path(), &none, &shell);
        assert!(found.contains(&"--amend".to_string()), "typed exactly, it is still there");
        let (found, _) = labels(&engine, "git commit --message one --", cwd.path(), &none, &shell);
        assert!(found.contains(&"--message".to_string()), "an option that takes a value may be given again: {found:?}");

        let c = engine.complete_in("ls -la", 6, cwd.path(), &none, &shell).unwrap();
        let h = c.candidates.iter().find(|c| c.label == "-h").unwrap_or_else(|| panic!("{:?}", c.candidates));
        assert_eq!(h.insert_text, "-lah ");
        assert!(c.candidates.iter().all(|c| c.label != "-l" && c.label != "-a"), "letters in the group are not offered again");
        let (found, _) = labels(&engine, "ls -la -", cwd.path(), &none, &shell);
        assert!(!found.contains(&"-l".to_string()) && found.contains(&"-h".to_string()), "nor after it: {found:?}");
        let hosts = |p: Provider, _: &std::path::Path| if p == Provider::SshHosts { vec![Suggestion { name: "box".into(), description: None, insert_value: None }] } else { vec![] };
        let (found, _) = labels(&engine, "ssh -v b", cwd.path(), &hosts, &shell);
        assert_eq!(found, ["box"]);
        let (found, _) = labels(&engine, "ssh -vp b", cwd.path(), &hosts, &shell);
        assert!(found.is_empty(), "the last letter of a group takes its value (a port, not a host): {found:?}");
    }

    #[test]
    fn a_dash_is_the_previous_branch_or_directory() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        let shell = ShellContext::default();
        for line in ["git checkout -", "git switch -", "cd -"] {
            let (found, _) = labels(&engine, line, cwd.path(), &none, &shell);
            assert_eq!(found.first().map(String::as_str), Some("-"), "{line:?}: {found:?}");
        }
        let (found, _) = labels(&engine, "git checkout ", cwd.path(), &none, &shell);
        assert!(!found.contains(&"-".to_string()), "only with the options: {found:?}");
        let (found, _) = labels(&engine, "defaults read -g", cwd.path(), &none, &shell);
        assert_eq!(found.first().map(String::as_str), Some("-globalDomain"), "any value starting with a dash: {found:?}");
    }

    #[test]
    fn git_aliases_read_as_what_they_stand_for() {
        let cwd = tempfile::tempdir().unwrap();
        let engine = Engine::new(bundled_specs());
        let values = |p: Provider, _: &std::path::Path| match p {
            Provider::GitAliases => vec![Suggestion { name: "cob".into(), description: Some("checkout -b".into()), insert_value: None },
                Suggestion { name: "c".into(), description: Some("commit".into()), insert_value: None }],
            Provider::GitBranches => vec![Suggestion { name: "main".into(), description: None, insert_value: None }],
            _ => vec![],
        };
        let shell = ShellContext::default();
        let (found, _) = labels(&engine, "git co", cwd.path(), &values, &shell);
        assert!(found.contains(&"cob".to_string()), "aliases are offered as subcommands: {found:?}");
        let (found, valid) = labels(&engine, "git c --am", cwd.path(), &values, &shell);
        assert!(valid && found.contains(&"--amend".to_string()), "{found:?}");
        let (found, valid) = labels(&engine, "git -C . c --am", cwd.path(), &values, &shell);
        assert!(valid && found.contains(&"--amend".to_string()), "after git's own options too: {found:?}");
    }

    #[test]
    fn cd_looks_in_cdpath_and_a_spec_can_need_a_file() {
        let cwd = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        std::fs::create_dir(projects.path().join("goh")).unwrap();
        let engine = Engine::new(bundled_specs());
        let shell = ShellContext { cdpath: vec![projects.path().to_string_lossy().into_owned()], ..Default::default() };
        let (found, _) = labels(&engine, "cd go", cwd.path(), &none, &shell);
        assert_eq!(found, ["goh/"]);
        let (found, _) = labels(&engine, "ls go", cwd.path(), &none, &shell);
        assert!(found.is_empty(), "only cd uses CDPATH");

        // `task` is go-task's only next to a Taskfile; elsewhere (Taskwarrior) the spec stays out of the way.
        let (_, valid) = labels(&engine, "task ", cwd.path(), &none, &shell);
        assert!(!valid);
        std::fs::write(cwd.path().join("Taskfile.yml"), "version: '3'\n").unwrap();
        let sub = cwd.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (_, valid) = labels(&engine, "task ", &sub, &none, &shell);
        assert!(valid);
    }
}
