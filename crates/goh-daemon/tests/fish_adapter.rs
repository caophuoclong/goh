//! adapters/fish/goh.fish in a real fish: `complete -C` asks for completions the way the Tab key does.
//! Skipped (with a note) when fish or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};

use common::{utf8_locale, Fixture};

/// fish on the test process's PATH, or in the usual Homebrew prefixes (a cargo run from an IDE may lack them).
fn fish() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join("fish")).find(|f| f.is_file())
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/fish/goh.fish")
}

#[test]
fn fish_completes_a_spec_command_through_goh() {
    let (Some(fish), Some(locale)) = (fish(), utf8_locale()) else {
        eprintln!("skipping: needs fish and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    // fish ships its own git completions and would answer with them whatever goh does: load them once (they load on
    // first use), erase them, then source the adapter so only goh's candidates can appear.
    let script = format!("complete -C 'git ' >/dev/null; complete -e -c git; source {}; complete -C 'git chec'", adapter().display());
    let out = fx.command(fish.to_str().unwrap(), &["--no-config", "-c", &script], &[]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "fish failed: {}", String::from_utf8_lossy(&out.stderr));
    // goh's spec describes it "Switch branches"; fish's own git completion says something else.
    assert!(text.lines().any(|l| l.starts_with("checkout") && l.contains("Switch branches")), "goh's candidate reaches fish: {text}");
}
