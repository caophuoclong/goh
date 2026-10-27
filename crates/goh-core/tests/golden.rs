use std::path::{Path, PathBuf};

use goh_core::{spec::Command, Engine, Response};
use serde_json::Value;

fn docs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(docs().join(rel)).unwrap()).unwrap()
}

fn assert_valid(schema: &str, instance: &Value) {
    let validator = jsonschema::validator_for(&json(schema)).unwrap();
    let errors: Vec<String> = validator.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{schema}: {errors:?}");
}

const SPECS: &[&str] = &[
    r#"{"schemaVersion":1,"name":"git",
        "options":[{"name":"--version"}],
        "subcommands":[
          {"name":"add","args":[{"name":"p","template":"filepaths","isVariadic":true}]},
          {"name":"check-ignore"},
          {"name":"checkout","args":[{"name":"branch","suggestions":[{"name":"main"}]}]},
          {"name":"cherry-pick"},
          {"name":"commit","options":[
            {"name":"--amend"},
            {"name":"-m","takesValue":true,"args":[{"name":"msg"}]}]},
          {"name":"fetch"}]}"#,
    r#"{"schemaVersion":1,"name":"docker","subcommands":[{"name":"run","options":[
          {"name":"--rm"},
          {"name":"--network","takesValue":true,"args":[{"name":"n","suggestions":[{"name":"host"},{"name":"bridge"}]}]}]}]}"#,
];

fn engine() -> Engine {
    Engine::new(SPECS.iter().map(|s| Command::from_json(s).unwrap()).collect())
}

/// Complete with the cursor at the end of `buffer`; returns labels in rank order.
fn labels(buffer: &str, cwd: &Path) -> Vec<String> {
    engine().complete(buffer, buffer.len(), cwd).unwrap().into_iter().map(|c| c.label).collect()
}

fn here() -> PathBuf {
    std::env::temp_dir()
}

#[test]
fn command_position() {
    assert_eq!(labels("gi", &here())[..1], ["git"]);
}

#[test]
fn exact_command_names_offer_the_next_word_without_replacing_the_command() {
    let engine = Engine::new(goh_core::bundled_specs());
    for (buffer, label, expected) in [
        ("docker", "ps", "docker ps "),
        ("git", "checkout", "git checkout "),
        ("npm", "install", "npm install "),
        ("echo Tài | docker", "ps", "echo Tài | docker ps "),
        ("docker ", "ps", "docker ps "),
    ] {
        let result = engine.complete_full(buffer, buffer.len(), &here()).unwrap();
        assert!(result.valid, "{buffer:?}");
        let candidate = result.candidates.iter().find(|c| c.label == label).unwrap_or_else(|| panic!("{buffer:?}: missing {label}"));
        assert_eq!((candidate.replace_start_byte, candidate.replace_end_byte), (buffer.len(), buffer.len()));
        let edited = format!("{}{}", buffer, candidate.insert_text);
        assert_eq!(edited, expected);
        assert!(candidate.match_ranges.is_empty());
    }
    for (buffer, cursor) in [("dock", 4), ("dockers", 7), ("dockerfile", 6), ("\"docker\"", 8), (r"d\ocker", 7)] {
        assert!(!engine.complete_full(buffer, cursor, &here()).unwrap().valid, "{buffer:?} at {cursor}");
    }
}

#[test]
fn subcommands_keep_spec_order() {
    assert_eq!(labels("git ", &here()), ["add", "check-ignore", "checkout", "cherry-pick", "commit", "fetch"]);
}

#[test]
fn prefix_then_in_order_matches() {
    assert_eq!(labels("git ch", &here()), ["check-ignore", "checkout", "cherry-pick", "fetch"]);
}

#[test]
fn options_after_subcommand_without_args() {
    assert_eq!(labels("git commit ", &here()), ["--amend", "-m"]);
    // Typed text present: equal scores tie-break alphabetically.
    assert_eq!(labels("docker run -", &here()), ["--network", "--rm"]);
}

#[test]
fn option_value_and_inline_value() {
    assert_eq!(labels("docker run --network ", &here()), ["host", "bridge"]);
    let r = engine().complete("docker run --network=ho", 23, &here()).unwrap();
    assert_eq!(r[0].label, "host");
    assert_eq!((r[0].replace_start_byte, r[0].replace_end_byte), (21, 23));
}

#[test]
fn option_value_is_not_a_subcommand() {
    // `-m` consumes the next word, so `ch` is the message, not a subcommand.
    assert!(labels("git commit -m ch", &here()).is_empty());
}

#[test]
fn replacement_span_covers_word_after_cursor() {
    let r = engine().complete("git checkout", 6, &here()).unwrap();
    let c = r.iter().find(|c| c.label == "checkout").unwrap();
    assert_eq!((c.replace_start_byte, c.replace_end_byte), (4, 12));
    assert_eq!(c.match_ranges, vec![[0, 2]]);
}

#[test]
fn rejects_bad_cursor_and_stays_quiet_on_unterminated_quote() {
    let e = engine();
    assert!(e.complete("é", 1, &here()).is_err());
    assert!(e.complete("git", 9, &here()).is_err());
    assert!(e.complete("git \"ch", 7, &here()).unwrap().is_empty());
}

#[test]
fn filesystem_accent_folding_and_escaping() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Tài liệu")).unwrap();
    std::fs::create_dir(dir.path().join("My Projects")).unwrap();
    std::fs::write(dir.path().join("My notes.txt"), "").unwrap();
    std::fs::write(dir.path().join(".hidden"), "").unwrap();

    let cd = engine().complete("cd tai", 6, dir.path()).unwrap();
    assert_eq!(cd.len(), 1);
    assert_eq!(cd[0].label, "Tài liệu/");
    assert_eq!(cd[0].insert_text, r"Tài\ liệu/");

    let cat = engine().complete("cat My", 6, dir.path()).unwrap();
    let inserts: Vec<_> = cat.iter().map(|c| c.insert_text.as_str()).collect();
    assert_eq!(inserts, [r"My\ Projects/", r"My\ notes.txt "]);

    assert!(labels("cat ", dir.path()).iter().all(|l| l != ".hidden"));
    assert!(labels("cat .", dir.path()).contains(&".hidden".to_string()));
    assert_eq!(labels("git add My", dir.path()).len(), 2);
}

#[test]
fn filesystem_keeps_directory_prefix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src/My Dir")).unwrap();
    let r = engine().complete("ls src/My", 9, dir.path()).unwrap();
    assert_eq!(r[0].insert_text, r"src/My\ Dir/");
    assert_eq!((r[0].replace_start_byte, r[0].replace_end_byte), (3, 9));
}

#[test]
fn examples_and_responses_match_schemas() {
    assert_valid("schemas/completion.schema.json", &json("examples/git.completion.json"));
    assert_valid("schemas/config.schema.json", &json("examples/config.json"));
    let specs = Path::new(env!("CARGO_MANIFEST_DIR")).join("specs");
    for entry in std::fs::read_dir(specs).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "json") {
            assert_valid("schemas/completion.schema.json", &serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap());
        }
    }
    assert_valid("schemas/ipc.schema.json", &json("examples/completion-response.json"));

    let git = Command::from_json(&std::fs::read_to_string(docs().join("examples/git.completion.json")).unwrap()).unwrap();
    let suggestions = Engine::new(vec![git]).complete("git ch", 6, &here()).unwrap();
    assert!(!suggestions.is_empty());
    let frame = serde_json::to_value(Response::new("session-0001".into(), 42, suggestions)).unwrap();
    assert_valid("schemas/ipc.schema.json", &frame);
}

// ---- validity: is this a recognised command in progress? --------------------------------------

fn valid(buffer: &str) -> bool {
    engine().complete_full(buffer, buffer.len(), &here()).unwrap().valid
}

#[test]
fn known_commands_with_placed_words_are_valid() {
    for line in ["git ", "git c", "git ch", "git checkout ", "git commit ", "git commit -", "git commit -m msg ", "docker run -", "docker run --network ho"] {
        assert!(valid(line), "{line:?} should be valid");
    }
}

#[test]
fn a_word_the_spec_has_no_place_for_makes_the_line_invalid() {
    // `foo` is not a git subcommand and git takes no arguments of its own.
    assert!(!valid("git foo c"));
    // `checkout` takes one argument; a second one has no slot.
    assert!(!valid("git checkout main extra "));
    // `commit` has no positional arguments in the spec.
    assert!(!valid("git commit oops "));
}

#[test]
fn unknown_commands_and_unfinished_commands_are_not_valid() {
    assert!(!valid("zzz c"), "no spec for zzz");
    assert!(!valid("gi"), "the command word itself is still being typed");
    assert!(!valid(""));
    assert!(!valid("git \"unterminated"), "unterminated quote");
}

#[test]
fn variadic_arguments_keep_the_line_valid() {
    assert!(valid("git add a b c d "), "git add takes any number of paths");
}

#[test]
fn an_option_value_is_not_an_unexpected_word() {
    assert!(valid("git commit -m some-message "));
    assert!(valid("docker run --network host "));
}

#[test]
fn explicit_completion_ignores_validity() {
    // Tab still works for a command with no spec (file names) and for a line the spec does not place.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "").unwrap();
    let r = engine().complete_full("cat no", 6, dir.path()).unwrap();
    assert!(!r.valid && r.candidates.iter().any(|c| c.label == "note.txt"));
}

#[test]
fn unasked_suggestions_prefer_prefix_matches_over_fuzzy_ones() {
    let r = engine().complete_full("git ch", 6, &here()).unwrap();
    let all: Vec<_> = r.candidates.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(all, ["check-ignore", "checkout", "cherry-pick", "fetch"], "explicit Tab keeps the fuzzy match");
    let only: Vec<_> = r.prefix_matches_only().candidates.into_iter().map(|c| c.label).collect();
    assert_eq!(only, ["check-ignore", "checkout", "cherry-pick"], "fetch (c..h in the middle) is dropped");
    // With no prefix match at all the fuzzy ones are all there is, so they stay.
    let r = engine().complete_full("git ec", 6, &here()).unwrap().prefix_matches_only();
    assert!(r.candidates.iter().any(|c| c.label == "fetch"), "{:?}", r.candidates.iter().map(|c| &c.label).collect::<Vec<_>>());
}

#[test]
fn one_typed_letter_lists_only_names_that_start_with_it() {
    assert_eq!(labels("git f", &here()), ["fetch"]);
    assert!(!labels("git c", &here()).contains(&"fetch".to_string()));
}

#[test]
fn options_are_not_matched_against_a_plain_word() {
    // `commit` takes no arguments, so only options could apply, but `am` is not an option.
    assert!(labels("git commit am", &here()).is_empty());
    assert_eq!(labels("git commit ", &here()), ["--amend", "-m"], "an empty word still lists them");
    assert_eq!(labels("git commit -a", &here()), ["--amend"], "a dash word matches options");
}

#[test]
fn deep_docker_arguments_use_fixed_runtime_values_and_keep_parent_flags_in_scope() {
    use goh_core::{protocol::{Kind, Source}, spec::{Provider, Suggestion}};
    let engine = Engine::new(goh_core::bundled_specs());
    let values = |provider: Provider, _: &Path| vec![Suggestion {
        name: match provider { Provider::DockerImages => "nginx:alpine", _ => "web-1" }.into(),
        description: Some("Local Docker data".into()), insert_value: None,
    }];
    for line in ["docker exec ", "docker exec -it ", "docker container exec -w /tmp "] {
        let result = engine.complete_with(line, line.len(), &here(), &values).unwrap();
        assert!(result.valid && result.candidates.iter().any(|c| c.label == "web-1" && c.source == Source::Generator), "{line}: {result:?}");
        assert!(result.candidates.iter().any(|c| c.label == "--interactive"), "discover flags even with positional args");
    }
    let line = "docker exec -it we";
    let result = engine.complete_with(line, line.len(), &here(), &values).unwrap();
    let web = result.candidates.iter().find(|c| c.label == "web-1").unwrap();
    assert_eq!(format!("{}{}", &line[..web.replace_start_byte], web.insert_text), "docker exec -it web-1 ");
    for line in ["docker exec web-1 ", "docker exec web-1 sh -c echo hello "] {
        let result = engine.complete_with(line, line.len(), &here(), &values).unwrap();
        assert!(result.valid && result.candidates.is_empty(), "inner commands cannot receive Docker's flags: {line}");
    }
    let line = "docker --context other exec w";
    let result = engine.complete_with(line, line.len(), &here(), &|_, _| panic!("must not query the wrong Docker context")).unwrap();
    assert!(result.valid && result.candidates.is_empty());
    let line = "docker run ng";
    assert!(engine.complete_with(line, line.len(), &here(), &values).unwrap().candidates.iter().any(|c| c.label == "nginx:alpine"));
    for (line, label) in [("docker compose up --", "--detach"), ("docker container exec --", "--interactive")] {
        let result = engine.complete_full(line, line.len(), &here()).unwrap();
        assert!(result.valid && result.candidates.iter().any(|c| c.label == label && c.kind == Kind::Option), "{line}");
    }
    let line = "go test ./... -r";
    assert!(engine.complete(line, line.len(), &here()).unwrap().iter().any(|c| c.label == "-run"));
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(cwd.path().join("app.js"), "").unwrap();
    assert!(engine.complete("node app", 8, cwd.path()).unwrap().iter().any(|c| c.label == "app.js"));
}

#[test]
fn cd_offers_frecent_directories_only_once_something_is_typed() {
    use goh_core::spec::{Provider, Suggestion};
    let engine = Engine::new(goh_core::bundled_specs());
    let values = |p: Provider, _: &Path| {
        assert_eq!(p, Provider::FrecentDirs);
        vec![Suggestion { name: "/work/apps/web components".into(), description: None, insert_value: None }]
    };
    let dir = tempfile::tempdir().unwrap();
    let r = engine.complete_with("cd webcomp", 10, dir.path(), &values).unwrap();
    assert_eq!(r.candidates[0].label, "/work/apps/web components");
    assert_eq!(r.candidates[0].insert_text, r"/work/apps/web\ components ");
    let r = engine.complete_with("cd ", 3, dir.path(), &|_, _| panic!("empty word must not query frecent dirs")).unwrap();
    assert!(r.candidates.is_empty());
}

#[test]
fn hosts_contexts_and_profiles_come_from_the_users_own_files() {
    use goh_core::spec::{Provider, Suggestion};
    let engine = Engine::new(goh_core::bundled_specs());
    let host = |name: &str, insert: Option<&str>| Suggestion { name: name.into(), description: Some("recent".into()), insert_value: insert.map(str::to_string) };
    let values = |p: Provider, _: &Path| match p {
        Provider::SshHosts => vec![host("alice@prod", None), host("staging", None)],
        Provider::SshRemotes => vec![host("prod", Some("prod:"))],
        Provider::KubeContexts => vec![host("dev-cluster", None)],
        Provider::AwsProfiles => vec![host("work", None)],
        _ => vec![],
    };
    let empty = tempfile::tempdir().unwrap(); // no files to compete with the provider's names
    let first = |line: &str| {
        let r = engine.complete_with(line, line.len(), empty.path(), &values).unwrap();
        let c = r.candidates.first().unwrap_or_else(|| panic!("nothing for {line:?}"));
        format!("{}{}", &line[..c.replace_start_byte], c.insert_text)
    };
    assert_eq!(first("ssh ali"), "ssh alice@prod ");
    assert_eq!(first("ssh -J stag"), "ssh -J staging ");
    assert_eq!(first("scp ./a.txt pr"), "scp ./a.txt prod:", "no space after the colon: the path comes next");
    assert_eq!(first("rsync -av pr"), "rsync -av prod:");
    assert_eq!(first("kubectl config use-context dev"), "kubectl config use-context dev-cluster ");
    assert_eq!(first("kubectl --context dev"), "kubectl --context dev-cluster ");
    assert_eq!(first("aws --profile wo"), "aws --profile work ");
    assert_eq!(first("helm --kube-context dev"), "helm --kube-context dev-cluster ");
}

#[test]
fn branches_remotes_scripts_and_targets_come_from_the_project() {
    use goh_core::spec::{Provider, Suggestion};
    let engine = Engine::new(goh_core::bundled_specs());
    let one = |name: &str| vec![Suggestion { name: name.into(), description: None, insert_value: None }];
    let values = |p: Provider, _: &Path| match p {
        Provider::GitBranches => one("feature/login"),
        Provider::GitRemotes => one("origin"),
        Provider::PackageScripts => one("dev-server"),
        Provider::MakeTargets => one("release"),
        _ => vec![],
    };
    let empty = tempfile::tempdir().unwrap();
    let first = |line: &str| {
        let r = engine.complete_with(line, line.len(), empty.path(), &values).unwrap();
        let c = r.candidates.first().unwrap_or_else(|| panic!("nothing for {line:?}"));
        format!("{}{}", &line[..c.replace_start_byte], c.insert_text)
    };
    for (typed, want) in [
        ("git checkout feat", "git checkout feature/login "),
        ("git switch feat", "git switch feature/login "),
        ("git merge feat", "git merge feature/login "),
        ("git rebase feat", "git rebase feature/login "),
        ("git branch -d feat", "git branch -d feature/login "),
        ("git push ori", "git push origin "),
        ("git pull origin feat", "git pull origin feature/login "),
        ("git fetch ori", "git fetch origin "),
        ("npm run dev", "npm run dev-server "),
        ("yarn run dev", "yarn run dev-server "),
        ("pnpm run dev", "pnpm run dev-server "),
        ("pnpm dev", "pnpm dev-server "),
        ("bun run dev", "bun run dev-server "),
        ("make rel", "make release "),
    ] {
        assert_eq!(first(typed), want, "{typed}");
    }
}

#[test]
fn an_alias_is_seen_through_and_spans_stay_in_the_line_as_typed() {
    use goh_core::shellctx::{NameKind, ShellContext};
    let engine = Engine::new(goh_core::bundled_specs());
    let ctx = ShellContext {
        aliases: [("gs".to_string(), "git status".to_string()), ("g".to_string(), "git".to_string())].into_iter().collect(),
        names: vec![("gs".to_string(), NameKind::Alias), ("g".to_string(), NameKind::Alias), ("ghostwriter".to_string(), NameKind::Executable)],
        ..Default::default()
    };
    let at = |line: &str| engine.complete_in(line, line.len(), &here(), &|_, _| vec![], &ctx).unwrap();
    let r = at("g ch");
    let c = r.candidates.iter().find(|c| c.label == "checkout").expect("completes as `git ch`");
    assert_eq!(format!("{}{}", &"g ch"[..c.replace_start_byte], c.insert_text), "g checkout ");
    assert!(r.valid, "the expanded line is a recognised command in progress");
    // Command position: the shell's own names, with a description saying what each is.
    let r = at("gho");
    let c = r.candidates.iter().find(|c| c.label == "ghostwriter").unwrap();
    assert_eq!((c.replace_start_byte, c.insert_text.as_str(), c.description.as_deref()), (0, "ghostwriter ", Some("command")));
    // Nothing shell-specific for an empty word: that would list every executable.
    assert!(at("").candidates.iter().all(|c| c.label != "ghostwriter"));
}

#[test]
fn a_slip_in_a_subcommand_or_option_is_corrected_only_when_nothing_else_matches() {
    let engine = Engine::new(goh_core::bundled_specs());
    let first = |line: &str| engine.complete_full(line, line.len(), &here()).unwrap().candidates.into_iter().next().map(|c| c.label);
    assert_eq!(first("git chekcout").as_deref(), Some("checkout"), "swapped letters");
    assert_eq!(first("git statsu").as_deref(), Some("status"));
    assert_eq!(first("git commit --amned").as_deref(), Some("--amend"));
    assert_eq!(first("dockre").as_deref(), Some("docker"), "the command word too");
    // Real matches win: nothing near is mixed in.
    let labels: Vec<_> = engine.complete_full("git stat", 8, &here()).unwrap().candidates.into_iter().map(|c| c.label).collect();
    assert!(labels.contains(&"status".to_string()) && !labels.contains(&"stash".to_string()), "{labels:?}");
    // File names are never corrected.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "").unwrap();
    let r = engine.complete_full("cat noets.txt", 13, dir.path()).unwrap();
    assert!(r.candidates.is_empty(), "{:?}", r.candidates);
}

#[test]
fn variables_complete_after_a_dollar_sign_and_for_export_and_unset() {
    use goh_core::shellctx::ShellContext;
    let engine = Engine::new(goh_core::bundled_specs());
    let ctx = ShellContext { env: vec!["HOME".into(), "HOSTNAME".into(), "PATH".into(), "bad name".into()], ..Default::default() };
    let at = |line: &str| engine.complete_in(line, line.len(), &here(), &|_, _| vec![], &ctx).unwrap().candidates;
    let apply = |line: &str, c: &goh_core::protocol::Candidate| format!("{}{}{}", &line[..c.replace_start_byte], c.insert_text, &line[c.replace_end_byte..]);
    let line = "cd $HO";
    let found = at(line);
    assert_eq!(found.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["HOME", "HOSTNAME"]);
    assert_eq!(apply(line, &found[0]), "cd $HOME", "no space: more path usually follows");
    assert_eq!(apply("echo ${PA", &at("echo ${PA")[0]), "echo ${PATH}");
    assert_eq!(apply("export HO", &at("export HO")[0]), "export HOME=", "export wants a value next");
    assert_eq!(apply("unset HOS", &at("unset HOS")[0]), "unset HOSTNAME ");
    assert!(at("export FOO=ba").is_empty() && at("echo \"$HO").is_empty(), "no names inside a value or a quote");
}

#[test]
fn extensions_narrow_files_but_not_directories() {
    let dir = tempfile::tempdir().unwrap();
    for f in ["app.py", "notes.txt", "data.TAR.GZ"] {
        std::fs::write(dir.path().join(f), "").unwrap();
    }
    std::fs::create_dir(dir.path().join("pkg")).unwrap();
    let spec = Command::from_json(r#"{"schemaVersion":1,"name":"run","args":[{"name":"f","template":"filepaths","extensions":["py","tar.gz"]}]}"#).unwrap();
    let engine = Engine::new(vec![spec]);
    let labels: Vec<_> = engine.complete("run ", 4, dir.path()).unwrap().into_iter().map(|c| c.label).collect();
    assert_eq!(labels, ["app.py", "data.TAR.GZ", "pkg/"]);
}

#[test]
fn values_used_before_come_first_for_an_option_and_never_for_other_slots() {
    let engine = Engine::new(vec![Command::from_json(
        r#"{"schemaVersion":1,"name":"kubectl","options":[{"name":"-n","takesValue":true,"args":[{"name":"ns","suggestions":[{"name":"default"}]}]}],"subcommands":[{"name":"get"}]}"#,
    ).unwrap()])
    .with_recall(Box::new(|command, option, _| if (command, option) == ("kubectl", "-n") { vec!["staging".into(), "prod eu".into()] } else { vec![] }));
    let at = |line: &str| engine.complete(line, line.len(), &here()).unwrap();
    let r = at("kubectl -n ");
    assert_eq!(r.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["staging", "prod eu", "default"]);
    assert_eq!(r[1].insert_text, "prod\\ eu ", "escaped like any value");
    assert_eq!(at("kubectl -n st")[0].label, "staging");
    assert!(at("kubectl ").iter().all(|c| c.label != "staging"), "only the option's own slot");
}
