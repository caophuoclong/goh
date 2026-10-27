//! `goh setup` against a scratch startup file: adds one block, changes nothing the second time, backs the file up once,
//! removes exactly the block, and refuses a shell it does not know.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn goh(args: &[&str], rc: &Path, shell: &str, path: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_goh"))
        .arg("setup")
        .args(args)
        .env_clear()
        .env("HOME", rc.parent().unwrap())
        .env("GOH_RC", rc)
        .env("GOH_SHELL", shell)
        .env("PATH", path)
        .output()
        .unwrap()
}

#[test]
fn setup_adds_and_removes_one_marked_block() {
    let dir = tempfile::tempdir().unwrap();
    let rc = dir.path().join("zshrc");
    let original = "export A=1\nalias g=git\n";
    fs::write(&rc, original).unwrap();
    let bin = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap().to_str().unwrap().to_string();

    // goh is on PATH: the block only loads the adapter.
    let out = goh(&[], &rc, "zsh", &bin);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = fs::read_to_string(&rc).unwrap();
    assert_eq!(text, format!("{original}\n# >>> goh >>>\neval \"$(goh init zsh)\"\n# <<< goh <<<\n"));
    assert_eq!(fs::read_to_string(dir.path().join("zshrc.goh-backup")).unwrap(), original);

    // A second run changes nothing.
    let again = goh(&[], &rc, "zsh", &bin);
    assert!(String::from_utf8_lossy(&again.stdout).contains("already has the goh block"));
    assert_eq!(fs::read_to_string(&rc).unwrap(), text);

    // Uninstall restores the file exactly.
    assert!(goh(&["--uninstall"], &rc, "zsh", &bin).status.success());
    assert_eq!(fs::read_to_string(&rc).unwrap(), original);
    assert!(String::from_utf8_lossy(&goh(&["--uninstall"], &rc, "zsh", &bin).stdout).contains("nothing to remove"));
}

#[test]
fn setup_adds_the_folder_to_path_when_goh_is_not_found_there_and_can_preview() {
    let dir = tempfile::tempdir().unwrap();
    let rc = dir.path().join("config.fish");
    let out = goh(&["--dry-run"], &rc, "fish", "/nonexistent");
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && shown.contains("fish_add_path") && shown.contains("goh init fish | source"), "{shown}");
    assert!(!rc.exists(), "a dry run writes nothing");
}

#[test]
fn setup_refuses_a_shell_it_cannot_load_by_itself() {
    let dir = tempfile::tempdir().unwrap();
    let out = goh(&[], &dir.path().join("rc"), "nu", "/usr/bin");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("pass --shell"));
}
