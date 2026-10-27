//! Filesystem provider: list entries for a path token and escape them for the shell.

use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 5000;

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

pub struct Listing {
    /// Directory part of the typed token, as typed (e.g. `src/`), re-used in insert text.
    pub dir_prefix: String,
    /// Basename part the user has typed so far.
    pub typed_name: String,
    pub entries: Vec<Entry>,
}

pub fn list(cwd: &Path, typed: &str, dirs_only: bool) -> Listing {
    let (dir_prefix, typed_name) = match typed.rfind('/') {
        Some(i) => typed.split_at(i + 1),
        None => ("", typed),
    };
    let dir = resolve_dir(cwd, dir_prefix);
    let show_hidden = typed_name.starts_with('.');

    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for e in read.flatten().take(MAX_ENTRIES) {
            let Ok(name) = e.file_name().into_string() else { continue };
            if name.starts_with('.') && !show_hidden {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                || std::fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false);
            if dirs_only && !is_dir {
                continue;
            }
            entries.push(Entry { name, is_dir });
        }
    }
    Listing { dir_prefix: dir_prefix.to_string(), typed_name: typed_name.to_string(), entries }
}

fn resolve_dir(cwd: &Path, dir_prefix: &str) -> PathBuf {
    if let Some(rest) = dir_prefix.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    cwd.join(dir_prefix)
}

/// Backslash-escape every character the shell could treat specially.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        let safe = c.is_alphanumeric()
            || matches!(c, '_' | '.' | '/' | '-' | '+' | ',' | ':' | '@' | '%' | '=')
            || (c == '~' && i == 0);
        if !safe {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_spaces_and_metacharacters() {
        assert_eq!(escape("My Projects/"), r"My\ Projects/");
        assert_eq!(escape("a&b$c"), r"a\&b\$c");
        assert_eq!(escape("Tài liệu"), r"Tài\ liệu");
        assert_eq!(escape("~/x"), "~/x");
    }
}
