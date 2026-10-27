//! `goh setup`: the one line that loads the adapter, added to your shell's startup file inside a marked block
//! (the same block `install.sh` writes), so a Homebrew or hand install needs one command instead of editing the
//! file. Nothing outside the block is touched, `--uninstall` removes exactly the block, and the file is backed up
//! once.

use std::path::{Path, PathBuf};

pub const BEGIN: &str = "# >>> goh >>>";
pub const END: &str = "# <<< goh <<<";

/// Shells that get a block (others load their adapter by hand: `goh init <shell>`).
pub fn supported(shell: &str) -> bool {
    matches!(shell, "zsh" | "bash" | "fish")
}

/// The startup file for a shell. `GOH_RC` overrides; bash falls back to `~/.bash_profile` where there is no `.bashrc`
/// on macOS.
pub fn rc_path(shell: &str, home: &Path, zdotdir: Option<&Path>, xdg_config: Option<&Path>, macos: bool) -> Option<PathBuf> {
    match shell {
        "zsh" => Some(zdotdir.unwrap_or(home).join(".zshrc")),
        "bash" => {
            let rc = home.join(".bashrc");
            Some(if macos && !rc.exists() { home.join(".bash_profile") } else { rc })
        }
        "fish" => Some(xdg_config.map_or_else(|| home.join(".config"), Path::to_path_buf).join("fish/config.fish")),
        _ => None,
    }
}

/// Is a program called `name` in one of the directories of `path`? Then the shell already finds it and no
/// `PATH` line is needed (Homebrew's `bin` is on it already). A resolved install path such as a versioned Cellar
/// directory must never be written into a startup file, and this avoids ever looking at one.
pub fn on_path(path: &str, name: &str) -> bool {
    path.split(':').filter(|d| d.starts_with('/')).any(|d| Path::new(d).join(name).is_file())
}

/// The block, ending in a newline. `bin_dir` is added to `PATH` only when given.
pub fn block(shell: &str, bin_dir: Option<&Path>) -> String {
    let mut out = format!("{BEGIN}\n");
    match (shell, bin_dir) {
        ("fish", dir) => {
            if let Some(d) = dir {
                out.push_str(&format!("fish_add_path {}\n", d.display()));
            }
            out.push_str("goh init fish | source\n");
        }
        (shell, dir) => {
            if let Some(d) = dir {
                let d = d.display();
                out.push_str(&format!("case \":$PATH:\" in *\":{d}:\"*) ;; *) export PATH=\"{d}:$PATH\" ;; esac\n"));
            }
            out.push_str(&format!("eval \"$(goh init {shell})\"\n"));
        }
    }
    out.push_str(END);
    out.push('\n');
    out
}

pub fn has_block(text: &str) -> bool {
    text.lines().any(|l| l == BEGIN)
}

/// `text` with the block appended, or `None` when it already has one.
pub fn add_block(text: &str, block: &str) -> Option<String> {
    if has_block(text) {
        return None;
    }
    let separator = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    Some(format!("{text}{separator}{block}"))
}

/// `text` without the block (and the blank line before it), or `None` when there is none.
pub fn remove_block(text: &str) -> Option<String> {
    if !has_block(text) {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line == BEGIN {
            inside = true;
            if out.last() == Some(&"") {
                out.pop();
            }
        } else if inside {
            inside = line != END;
        } else {
            out.push(line);
        }
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_loads_the_adapter_and_adds_to_path_only_when_asked() {
        assert_eq!(block("zsh", None), "# >>> goh >>>\neval \"$(goh init zsh)\"\n# <<< goh <<<\n");
        let with = block("bash", Some(Path::new("/home/me/.local/bin")));
        assert!(with.contains("export PATH=\"/home/me/.local/bin:$PATH\"") && with.contains("goh init bash"), "{with}");
        assert_eq!(block("fish", None), "# >>> goh >>>\ngoh init fish | source\n# <<< goh <<<\n");
        assert!(block("fish", Some(Path::new("/x/bin"))).contains("fish_add_path /x/bin"));
    }

    #[test]
    fn adding_is_idempotent_and_keeps_everything_else_byte_for_byte() {
        let b = block("zsh", None);
        let original = "export A=1\nalias g=git\n";
        let added = add_block(original, &b).unwrap();
        assert_eq!(added, format!("{original}\n{b}"));
        assert_eq!(add_block(&added, &b), None, "a second run changes nothing");
        assert_eq!(remove_block(&added).unwrap(), original, "removal restores the file exactly");
        assert_eq!(add_block("", &b).unwrap(), b);
        assert_eq!(add_block("no newline", &b).unwrap(), format!("no newline\n\n{b}"));
        assert_eq!(remove_block(original), None);
    }

    #[test]
    fn removal_leaves_text_after_the_block_alone() {
        let b = block("zsh", None);
        let text = format!("a\n\n{b}b\n");
        assert_eq!(remove_block(&text).unwrap(), "a\nb\n");
    }

    #[test]
    fn startup_files_per_shell() {
        let home = Path::new("/h");
        assert_eq!(rc_path("zsh", home, None, None, true), Some(PathBuf::from("/h/.zshrc")));
        assert_eq!(rc_path("zsh", home, Some(Path::new("/z")), None, true), Some(PathBuf::from("/z/.zshrc")));
        assert_eq!(rc_path("fish", home, None, None, false), Some(PathBuf::from("/h/.config/fish/config.fish")));
        assert_eq!(rc_path("nu", home, None, None, false), None);
        assert!(!supported("nu") && supported("fish"));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(rc_path("bash", dir.path(), None, None, true), Some(dir.path().join(".bash_profile")), "macOS without .bashrc");
        std::fs::write(dir.path().join(".bashrc"), "").unwrap();
        assert_eq!(rc_path("bash", dir.path(), None, None, true), Some(dir.path().join(".bashrc")));
    }

    #[test]
    fn a_program_in_a_path_directory_needs_no_path_line() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("goh"), "").unwrap();
        let path = format!("/nonexistent:{}:relative", dir.path().display());
        assert!(on_path(&path, "goh") && !on_path(&path, "other") && !on_path("", "goh"));
    }
}
