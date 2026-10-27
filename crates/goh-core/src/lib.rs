//! Headless completion engine (docs/06). Knows nothing about UI, shell or OS:
//! `(buffer, cursor, cwd)` in, ranked candidates with replacement spans out.
//! It never executes user text.

mod fsys;
mod matching;
pub mod protocol;
pub mod spec;
mod tokenize;

use std::path::Path;

use protocol::{Candidate, Kind, Source};
use spec::{Arg, Command, Opt, Template};

pub use protocol::Response;

const MAX_RESULTS: usize = 50;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    CursorOutOfRange,
    CursorNotOnCharBoundary,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CursorOutOfRange => f.write_str("cursor is past the end of the buffer"),
            Error::CursorNotOnCharBoundary => f.write_str("cursor is not on a UTF-8 character boundary"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Engine {
    specs: Vec<Command>,
}

impl Engine {
    pub fn new(specs: Vec<Command>) -> Self {
        Self { specs }
    }

    pub fn complete(&self, buffer: &str, cursor_byte: usize, cwd: &Path) -> Result<Vec<Candidate>, Error> {
        Ok(self.complete_full(buffer, cursor_byte, cwd)?.candidates)
    }

    /// Candidates plus whether the line so far is *valid* (see [`Completion::valid`]).
    pub fn complete_full(&self, buffer: &str, cursor_byte: usize, cwd: &Path) -> Result<Completion, Error> {
        if cursor_byte > buffer.len() {
            return Err(Error::CursorOutOfRange);
        }
        if !buffer.is_char_boundary(cursor_byte) {
            return Err(Error::CursorNotOnCharBoundary);
        }
        let Some(parsed) = tokenize::parse(&buffer[..cursor_byte]) else {
            return Ok(Completion { candidates: vec![], valid: false });
        };

        // Replace the whole word under the cursor, including the part after it.
        let cur = &parsed.current;
        let end = cursor_byte + tokenize::word_tail(&buffer[cursor_byte..]);
        let mut sink = Sink::new(cur.value.clone(), cur.start, end, cwd);
        let raw_is_plain = !cur.quoted && cur.value.len() == cursor_byte - cur.start;
        let mut valid = false;

        match parsed.words.first() {
            None => {
                for s in &self.specs {
                    sink.push(&s.name, s.description.as_deref(), Kind::Command, format!("{} ", s.name), Source::Bundled, &s.name);
                }
                if raw_is_plain && cur.value.contains('/') {
                    sink.files(false);
                }
            }
            Some(first) => {
                let argv: Vec<&str> = parsed.words[1..].iter().map(|t| t.value.as_str()).collect();
                match self.specs.iter().find(|s| s.name == first.value) {
                    Some(spec) => valid = complete_known(&mut sink, spec, &argv, raw_is_plain),
                    None if !cur.quoted => sink.files(first.value == "cd"),
                    None => {}
                }
            }
        }
        Ok(Completion { candidates: sink.finish(), valid })
    }
}

/// What the engine found for a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub candidates: Vec<Candidate>,
    /// The line is a *recognised command in progress*: its first word has a completion spec, and every
    /// word before the one being typed fits that spec (a subcommand that exists, or a value an option or
    /// the command's arguments can take). Unknown commands, a word the spec has no place for
    /// (`git foo c`, `git checkout main extra c`) and unterminated quotes are not valid. Options are
    /// not checked (specs list only some of them), and a command still being typed (`gi`) is not valid
    /// yet. Used to decide whether to offer suggestions without being asked (as-you-type mode); an
    /// explicit Tab does not depend on it.
    pub valid: bool,
}

impl Completion {
    /// For suggestions nobody asked for: when anything matches by prefix, drop the scattered in-order
    /// (fuzzy) matches, which are useful when you ask but noise when they appear on their own.
    pub fn prefix_matches_only(mut self) -> Self {
        const PREFIX_SCORE: f64 = 90.0; // case-folded prefix and exact prefix; fuzzy scores are below 50
        if self.candidates.iter().any(|c| c.score >= PREFIX_SCORE) {
            self.candidates.retain(|c| c.score >= PREFIX_SCORE);
        }
        self
    }
}

/// Walk `argv` through the spec tree to find where the current word sits. Returns whether every word
/// before the current one is placed by the spec (see [`Completion::valid`]).
fn complete_known(sink: &mut Sink, root: &Command, argv: &[&str], raw_is_plain: bool) -> bool {
    let mut valid = true;
    let mut node = root;
    let mut pending: Option<&Opt> = None;
    let mut positional = 0;
    for &w in argv {
        if pending.take().is_some() {
            continue; // value of the previous option
        }
        if w.len() > 1 && w.starts_with('-') {
            let (name, inline) = split_eq(w);
            if let Some(opt) = find_opt(node, name) {
                if opt.takes_value && inline.is_none() {
                    pending = Some(opt);
                }
            }
            continue;
        }
        if positional == 0 {
            if let Some(sub) = node.subcommands.iter().find(|c| c.name == w) {
                node = sub;
                continue;
            }
        }
        // A word that is neither a subcommand nor fits an argument slot has no place in the spec.
        let has_slot = node.args.get(positional).is_some() || node.args.last().is_some_and(|a| a.is_variadic);
        if !has_slot {
            valid = false;
        }
        positional += 1;
    }

    let typed = sink.typed.clone();
    let scope = root.name.as_str();

    if let Some(opt) = pending {
        sink.arg_values(opt.args.first(), scope);
    } else if typed.starts_with('-') {
        let (name, inline) = split_eq(&typed);
        match (inline, find_opt(node, name)) {
            (Some(_), Some(opt)) if opt.takes_value && raw_is_plain => {
                sink.skip_to_value(name.len() + 1);
                sink.arg_values(opt.args.first(), scope);
            }
            (Some(_), _) => {}
            (None, _) => sink.options(node, scope),
        }
    } else {
        for sub in &node.subcommands {
            if positional == 0 {
                sink.push(&sub.name, sub.description.as_deref(), Kind::Subcommand, format!("{} ", sub.name), Source::Bundled, scope);
            }
        }
        let arg = node.args.get(positional).or_else(|| node.args.last().filter(|a| a.is_variadic));
        sink.arg_values(arg, scope);
        // Options are offered for an empty word (to discover them) or one starting with `-` (the branch above),
        // never for a plain word such as a remote name: matching `origin` against `--force-with-lease` is noise.
        if node.subcommands.is_empty() && node.args.is_empty() && typed.is_empty() {
            sink.options(node, scope);
        }
    }
    valid
}

fn split_eq(word: &str) -> (&str, Option<&str>) {
    match word.split_once('=') {
        Some((n, v)) => (n, Some(v)),
        None => (word, None),
    }
}

fn find_opt<'a>(node: &'a Command, name: &str) -> Option<&'a Opt> {
    node.options.iter().find(|o| o.name == name)
}

/// Collects matching candidates for the word under the cursor.
struct Sink<'a> {
    typed: String,
    start: usize,
    end: usize,
    cwd: &'a Path,
    out: Vec<Candidate>,
}

impl<'a> Sink<'a> {
    fn new(typed: String, start: usize, end: usize, cwd: &'a Path) -> Self {
        Self { typed, start, end, cwd, out: Vec::new() }
    }

    /// Complete only the part after `--opt=`.
    fn skip_to_value(&mut self, skip: usize) {
        self.typed.drain(..skip);
        self.start += skip;
    }

    fn push(&mut self, label: &str, description: Option<&str>, kind: Kind, insert_text: String, source: Source, scope: &str) {
        let Some(m) = matching::match_label(&self.typed, label) else { return };
        self.out.push(Candidate {
            id: format!("{scope}:{label}"),
            label: label.to_string(),
            description: description.map(str::to_string),
            kind,
            insert_text,
            replace_start_byte: self.start,
            replace_end_byte: self.end,
            match_ranges: m.ranges,
            score: m.score,
            source,
        });
    }

    fn options(&mut self, node: &Command, scope: &str) {
        for o in &node.options {
            self.push(&o.name, o.description.as_deref(), Kind::Option, format!("{} ", o.name), Source::Bundled, scope);
        }
    }

    fn arg_values(&mut self, arg: Option<&Arg>, scope: &str) {
        let Some(arg) = arg else { return };
        for s in &arg.suggestions {
            let insert = s.insert_value.clone().unwrap_or_else(|| fsys::escape(&s.name));
            self.push(&s.name, s.description.as_deref(), Kind::Argument, format!("{insert} "), Source::Bundled, scope);
        }
        match arg.template {
            Some(Template::Filepaths) => self.files(false),
            Some(Template::Folders) => self.files(true),
            None => {}
        }
    }

    fn files(&mut self, dirs_only: bool) {
        let listing = fsys::list(self.cwd, &self.typed, dirs_only);
        // Match against the basename only; the directory part is kept verbatim.
        let full_typed = std::mem::replace(&mut self.typed, listing.typed_name);
        for e in &listing.entries {
            let label = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
            let (kind, tail) = if e.is_dir { (Kind::Directory, "") } else { (Kind::File, " ") };
            let insert = format!("{}{tail}", fsys::escape(&format!("{}{label}", listing.dir_prefix)));
            self.push(&label, None, kind, insert, Source::Filesystem, "fs");
        }
        self.typed = full_typed;
    }

    fn finish(mut self) -> Vec<Candidate> {
        let sort_by_label = !self.typed.is_empty();
        // Stable: with nothing typed, spec order is kept.
        self.out.sort_by(|a, b| {
            b.score.total_cmp(&a.score).then_with(|| if sort_by_label { a.label.cmp(&b.label) } else { std::cmp::Ordering::Equal })
        });
        let mut seen = std::collections::HashSet::new();
        self.out.retain(|c| seen.insert((c.kind as u8, c.insert_text.clone(), c.replace_start_byte, c.replace_end_byte)));
        self.out.truncate(MAX_RESULTS);
        self.out
    }
}
