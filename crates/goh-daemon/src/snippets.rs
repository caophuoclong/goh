//! Your own shortcuts, `snippets.json` next to the config file: a name that expands to a command line, with
//! `{{cursor}}` marking where the cursor lands.
//!
//! ```json
//! [{ "name": "gcm", "template": "git commit -m \"{{cursor}}\"", "description": "Commit with a message" }]
//! ```
//! Typed at the start of a line, `gc` offers `gcm`; accepting it replaces the word with the template.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use goh_core::protocol::{Candidate, Kind, Source};
use serde::Deserialize;

pub const CURSOR: &str = "{{cursor}}";
const MAX_SNIPPETS: usize = 200;
const MAX_TEMPLATE: usize = 500;
const TTL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn path() -> Option<PathBuf> {
    goh_theme::config_path().map(|p| p.with_file_name("snippets.json"))
}

fn usable(s: &Snippet) -> bool {
    (1..=40).contains(&s.name.len())
        && s.name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !s.name.starts_with('-')
        && !s.template.is_empty()
        && s.template.len() <= MAX_TEMPLATE
        && !s.template.chars().any(char::is_control)
        && s.template.matches(CURSOR).count() <= 1
        && s.description.as_deref().is_none_or(|d| d.len() <= 200 && !d.chars().any(char::is_control))
}

/// The usable snippets in a file's text; entries that are not are left out (a broken file is no snippets).
pub fn parse(text: &str) -> Vec<Snippet> {
    serde_json::from_str::<Vec<Snippet>>(text).map(|v| v.into_iter().filter(usable).take(MAX_SNIPPETS).collect()).unwrap_or_default()
}

/// Template with the marker taken out, and where the marker was.
fn expand(template: &str) -> (String, Option<usize>) {
    match template.find(CURSOR) {
        Some(at) => (template.replacen(CURSOR, "", 1), Some(at)),
        None => (template.to_string(), None),
    }
}

/// Snippets whose name starts with the word being typed at the start of the line. As-you-type asks for two
/// characters first; a Tab asks for one.
pub fn candidates(snippets: &[Snippet], buffer: &str, cursor: usize, live: bool) -> Vec<Candidate> {
    let typed = &buffer[..cursor];
    if typed.chars().count() < if live { 2 } else { 1 } || typed.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\')) {
        return vec![];
    }
    let end = cursor + buffer[cursor..].find(char::is_whitespace).unwrap_or(buffer.len() - cursor);
    snippets
        .iter()
        .filter(|s| s.name.starts_with(typed))
        .map(|s| {
            let (insert_text, cursor_offset) = expand(&s.template);
            let shown = s.description.clone().unwrap_or_else(|| s.template.replace(CURSOR, "…"));
            Candidate {
                id: format!("snippet:{}", s.name),
                label: s.name.clone(),
                description: Some(shown),
                kind: Kind::Command,
                insert_text,
                replace_start_byte: 0,
                replace_end_byte: end,
                match_ranges: vec![[0, typed.len()]],
                score: 100.0,
                source: Source::Local,
                cursor_offset,
            }
        })
        .collect()
}

/// `snippets.json`, re-read at most every couple of seconds.
#[derive(Default)]
pub struct SnippetCache {
    inner: Mutex<Option<(Instant, Arc<Vec<Snippet>>)>>,
}

impl SnippetCache {
    pub fn get(&self) -> Arc<Vec<Snippet>> {
        let Ok(mut cached) = self.inner.lock() else { return Arc::default() };
        if let Some((at, list)) = cached.as_ref().filter(|(at, _)| at.elapsed() < TTL) {
            let _ = at;
            return Arc::clone(list);
        }
        let list = Arc::new(path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse(&t)).unwrap_or_default());
        *cached = Some((Instant::now(), Arc::clone(&list)));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Snippet> {
        parse(r#"[
          {"name":"gcm","template":"git commit -m \"{{cursor}}\"","description":"Commit with a message"},
          {"name":"dr","template":"docker run --rm -it {{cursor}} sh"},
          {"name":"two","template":"{{cursor}} {{cursor}}"},
          {"name":"bad name","template":"x"},
          {"name":"ctl","template":"a\nb"}
        ]"#)
    }

    #[test]
    fn only_usable_snippets_are_kept() {
        assert_eq!(list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gcm", "dr"]);
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn a_snippet_replaces_the_first_word_and_puts_the_cursor_at_its_marker() {
        let found = candidates(&list(), "gc", 2, true);
        assert_eq!(found.len(), 1);
        let c = &found[0];
        assert_eq!((c.insert_text.as_str(), c.cursor_offset, c.replace_start_byte, c.replace_end_byte), ("git commit -m \"\"", Some(15), 0, 2));
        assert_eq!(c.description.as_deref(), Some("Commit with a message"));
        // The rest of the word under the cursor is replaced too; a template without a marker ends the cursor at its end.
        let c = &candidates(&list(), "dr", 1, false)[0];
        assert_eq!((c.replace_end_byte, c.cursor_offset), (2, Some(20)));
        assert_eq!(c.description.as_deref(), Some("docker run --rm -it … sh"));
    }

    #[test]
    fn snippets_apply_to_the_first_word_only_and_wait_for_two_characters_while_typing() {
        assert!(candidates(&list(), "g", 1, true).is_empty(), "one character while typing is too few");
        assert_eq!(candidates(&list(), "g", 1, false).len(), 1, "but a Tab asks for it");
        assert!(candidates(&list(), "git gc", 6, true).is_empty());
        assert!(candidates(&list(), "", 0, false).is_empty());
        assert!(candidates(&list(), "xg", 2, true).is_empty());
    }
}
