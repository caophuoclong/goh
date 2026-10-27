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
    for name in ["git", "npm", "docker"] {
        assert_valid("schemas/completion.schema.json", &json(&format!("examples/specs/{name}.json")));
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
