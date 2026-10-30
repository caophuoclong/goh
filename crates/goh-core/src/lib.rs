//! Headless completion engine (docs/06). Knows nothing about UI, shell or OS:
//! `(buffer, cursor, cwd)` in, ranked candidates with replacement spans out.
//! It never executes user text.

mod bundled;
mod fsys;
mod matching;
pub mod protocol;
pub mod shellctx;
pub mod spec;
mod tokenize;

use std::collections::HashSet;
use std::path::Path;
use std::sync::RwLock;

use protocol::{Candidate, Kind, Source};
use shellctx::{NameKind, ShellContext};
use spec::{Arg, Command, Opt, Provider, Suggestion, Template};

pub use bundled::bundled_specs;
pub use protocol::Response;

const MAX_RESULTS: usize = 50;

/// Values given before for an option of a command: `(command, option, cwd)` -> values, best first.
pub type RecallFn = dyn Fn(&str, &str, &Path) -> Vec<String> + Send + Sync;

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
    specs: RwLock<Vec<Command>>,
    recall: Option<Box<RecallFn>>,
}

impl Engine {
    pub fn new(specs: Vec<Command>) -> Self {
        Self { specs: RwLock::new(specs), recall: None }
    }

    /// Is there a spec for this command?
    pub fn knows(&self, command: &str) -> bool {
        self.specs().iter().any(|s| s.name == command)
    }

    /// Serve a spec found while running (never replaces one that exists).
    pub fn add_spec(&self, spec: Command) {
        if let Ok(mut specs) = self.specs.write() {
            if !specs.iter().any(|s| s.name == spec.name) {
                specs.push(spec);
            }
        }
    }

    fn specs(&self) -> std::sync::RwLockReadGuard<'_, Vec<Command>> {
        self.specs.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Where values used before for an option come from (the host decides whether it remembers any).
    pub fn with_recall(mut self, recall: Box<RecallFn>) -> Self {
        self.recall = Some(recall);
        self
    }

    pub fn complete(&self, buffer: &str, cursor_byte: usize, cwd: &Path) -> Result<Vec<Candidate>, Error> {
        Ok(self.complete_full(buffer, cursor_byte, cwd)?.candidates)
    }

    /// Candidates plus whether the line so far is *valid* (see [`Completion::valid`]).
    pub fn complete_full(&self, buffer: &str, cursor_byte: usize, cwd: &Path) -> Result<Completion, Error> {
        self.complete_with(buffer, cursor_byte, cwd, &|_, _| vec![])
    }

    /// The host supplies fixed read-only provider data; this engine never starts a process.
    pub fn complete_with(&self, buffer: &str, cursor_byte: usize, cwd: &Path,
        values: &dyn Fn(Provider, &Path) -> Vec<Suggestion>) -> Result<Completion, Error> {
        self.complete_in(buffer, cursor_byte, cwd, values, &ShellContext::default())
    }

    /// Like [`Engine::complete_with`], also using what the shell knows: aliases are seen through (the
    /// spans returned are still in the line as typed) and the command word completes from its names.
    pub fn complete_in(&self, buffer: &str, cursor_byte: usize, cwd: &Path,
        values: &dyn Fn(Provider, &Path) -> Vec<Suggestion>, shell: &ShellContext) -> Result<Completion, Error> {
        if cursor_byte > buffer.len() {
            return Err(Error::CursorOutOfRange);
        }
        if !buffer.is_char_boundary(cursor_byte) {
            return Err(Error::CursorNotOnCharBoundary);
        }
        // An alias is seen through only towards a command with a spec: `alias cd=z` (zoxide's function) keeps `cd`'s.
        let has_spec = |line: &str| line.split_whitespace().next().is_some_and(|w| self.specs().iter().any(|s| s.name == w));
        if let Some(e) = shell.expand(buffer, cursor_byte).filter(|e| has_spec(&e.buffer) || !has_spec(buffer)) {
            let mut c = self.complete_in(&e.buffer, e.cursor, cwd, values, &ShellContext::default())?;
            for cand in &mut c.candidates {
                // Candidates replace the word being typed, which lies after the alias: shift back.
                cand.replace_start_byte = (cand.replace_start_byte as isize - e.delta).max(0) as usize;
                cand.replace_end_byte = (cand.replace_end_byte as isize - e.delta).max(0) as usize;
            }
            return Ok(c);
        }
        let specs = self.specs();
        let Some(parsed) = tokenize::parse(&buffer[..cursor_byte]) else {
            return Ok(Completion { candidates: vec![], valid: false });
        };

        // Replace the whole word under the cursor, including the part after it.
        let cur = &parsed.current;
        let end = cursor_byte + tokenize::word_tail(&buffer[cursor_byte..]);
        let raw_is_plain = !cur.quoted && cur.value.len() == cursor_byte - cur.start;
        let env = Env { cwd, values, recall: self.recall.as_deref(), specs: &specs, shell, plain: raw_is_plain, quoted: cur.quoted };
        let mut sink = Sink::new(cur.value.clone(), cur.start, end, env);
        let mut valid = false;

        // `FOO=1 git ch`: assignments before the command are its environment; the command comes after them.
        let words: Vec<&str> = parsed.words.iter().map(|t| t.value.as_str()).skip_while(|w| is_assignment(w)).collect();
        match words.split_first() {
            None => {
                if let Some(spec) = spec_for(&specs, &cur.value, cwd).filter(|_| raw_is_plain && cursor_byte == buffer.len()) {
                    // An exact command at end of line is ready for its next word, even before a
                    // space is typed. Insert after it, including the missing separator in the edit.
                    sink = Sink::new(String::new(), cursor_byte, cursor_byte, env);
                    valid = complete_known(&mut sink, spec, &[]);
                    for candidate in sink.out.iter_mut().chain(&mut sink.near) {
                        candidate.insert_text.insert(0, ' ');
                    }
                } else {
                    sink.command_names();
                }
            }
            Some((&first, argv)) => {
                match spec_for(&specs, first, cwd) {
                    Some(spec) => valid = complete_known(&mut sink, spec, argv),
                    // Variable names are all `export` and `unset` take; no spec is needed to know that.
                    None if raw_is_plain && argv.is_empty() && !cur.value.contains('=') && matches!(first, "export" | "unset" | "printenv") => {
                        let tail = if first == "export" { "=" } else { " " };
                        sink.env_names(0, tail);
                        valid = true;
                    }
                    None if !cur.quoted => sink.files(first == "cd", &[]),
                    None => {}
                }
            }
        }
        // `$NA` completes to a variable of the shell, wherever it stands in the line.
        if raw_is_plain {
            if let Some(skip) = ["${", "$"].iter().find(|p| cur.value.starts_with(**p)).map(|p| p.len()) {
                let tail = if skip == 2 { "}" } else { "" };
                sink.out.clear();
                sink.near.clear();
                sink.env_names(skip, tail);
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

/// How many times a word may be replaced by a subcommand alias in one line (an alias can name another).
const MAX_ALIAS_EXPANSIONS: usize = 5;

/// Walk `argv` through the spec tree to find where the current word sits. Returns whether every word
/// before the current one is placed by the spec (see [`Completion::valid`]).
fn complete_known(sink: &mut Sink, root: &Command, argv: &[&str]) -> bool {
    let mut argv: Vec<String> = argv.iter().map(|w| w.to_string()).collect();
    let mut valid = true;
    let mut node = root;
    let mut pending: Option<&Opt> = None;
    let mut positional = 0;
    let mut end_options = false;
    let mut command_started = false;
    // Options already given to this subcommand: not offered again unless they can be.
    let mut used: HashSet<&str> = HashSet::new();
    let mut expansions = 0;
    let mut i = 0;
    while i < argv.len() {
        let w = argv[i].clone();
        i += 1;
        if pending.take().is_some() {
            continue; // value of the previous option
        }
        if command_started { continue; }
        if !end_options && w == "--" { end_options = true; continue; }
        let slot = node.args.get(positional).or_else(|| node.args.last().filter(|a| a.is_variadic));
        if slot.is_some_and(|a| a.is_command) { command_started = true; continue; }
        if !end_options && w.len() > 1 && w.starts_with('-') {
            let (name, inline) = split_eq(&w);
            if let Some(opt) = find_opt(node, name) {
                used.insert(&opt.name);
                if opt.takes_value && inline.is_none() {
                    pending = Some(opt);
                }
            } else if let Some(bundle) = short_bundle(node, &w) {
                used.extend(bundle.iter().map(|o| o.name.as_str()));
                // Its last letter may take a value: `tar -xzf archive.tgz`.
                pending = bundle.last().copied().filter(|o| o.takes_value);
            }
            continue;
        }
        if let Some(slot) = slot.filter(|a| a.runs_command) {
            // `sudo git ch`, `env FOO=1 git ch`: the rest of the line is another command, completed by its own spec.
            let rest: Vec<&str> = argv[i - 1..].iter().map(String::as_str).skip_while(|w| is_assignment(w)).collect();
            return match rest.split_first() {
                None => {
                    sink.arg_values(Some(slot), &root.name);
                    valid
                }
                Some((command, rest)) => match spec_for(sink.env.specs, command, sink.env.cwd) {
                    Some(spec) => complete_known(sink, spec, rest) && valid,
                    None => {
                        if !sink.env.quoted {
                            sink.files(false, &[]);
                        }
                        false
                    }
                },
            };
        }
        if positional == 0 {
            if let Some(sub) = node.subcommands.iter().find(|c| c.name == w) {
                node = sub;
                used.clear();
                continue;
            }
            // `git co ma` with `co = checkout` in git's config reads on as `git checkout ma`.
            let expansion = node.subcommand_aliases.filter(|_| expansions < MAX_ALIAS_EXPANSIONS).and_then(|p| sink.alias(p, &w));
            if let Some(expansion) = expansion {
                expansions += 1;
                i -= 1;
                argv.splice(i..=i, expansion);
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

    // An option still waiting for its value does not yet say where the command acts.
    let given = if pending.is_some() { &argv[..argv.len() - 1] } else { &argv[..] };
    sink.providers_enabled = !targets_elsewhere(&root.name, given);
    sink.cdpath = matches!(root.name.as_str(), "cd" | "pushd");

    let typed = sink.typed.clone();
    let scope = root.name.as_str();

    // A delegated command (e.g. the program inside a container) owns all following flags.
    // We cannot inspect that executable here, so never present parent Docker flags as its options.
    let arg = node.args.get(positional).or_else(|| node.args.last().filter(|a| a.is_variadic));
    if command_started || arg.is_some_and(|a| a.is_command) { return valid; }
    if let Some(opt) = pending {
        sink.recalled(scope, &opt.name);
        sink.arg_values(opt.args.first(), scope);
    } else if !end_options && typed.starts_with('-') {
        let (name, inline) = split_eq(&typed);
        match (inline, find_opt(node, name)) {
            (Some(_), Some(opt)) if opt.takes_value && sink.env.plain => {
                sink.skip_to_value(name.len() + 1);
                sink.recalled(scope, &opt.name);
                sink.arg_values(opt.args.first(), scope);
            }
            (Some(_), _) => {}
            (None, found) => {
                // `tar -xz` is a group of single-letter options, unless it starts the name of one (`find -na`).
                let starts_one = node.options.iter().any(|o| o.name.starts_with(typed.as_str()));
                match short_bundle(node, &typed).filter(|_| found.is_none() && !starts_one) {
                    Some(bundle) if !bundle.last().is_some_and(|o| o.takes_value) => sink.bundle(node, &bundle, &used, scope),
                    Some(_) => {} // its last letter waits for a value
                    None => {
                        sink.options(node, &used, scope);
                        // Values that start with a dash: `git checkout -` (the previous branch), `defaults read -g`.
                        sink.dash_values(arg, scope);
                    }
                }
            }
        }
    } else {
        if positional == 0 {
            for sub in &node.subcommands {
                sink.push(&sub.name, sub.description.as_deref(), Kind::Subcommand, format!("{} ", sub.name), Source::Bundled, scope);
            }
            if let Some(provider) = node.subcommand_aliases {
                sink.aliases(provider, scope);
            }
        }
        sink.arg_values(arg, scope);
        // Options are offered for an empty word (to discover them) or one starting with `-` (the branch above),
        // never for a plain word such as a remote name: matching `origin` against `--force-with-lease` is noise.
        if !end_options && node.subcommands.is_empty() && typed.is_empty() {
            sink.options(node, &used, scope);
        }
    }
    valid
}

fn is_identifier(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `NAME=value`, which sets a variable for the command that follows it.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| is_identifier(name))
}

/// The spec for `name`, unless it applies only next to a file that is not here.
fn spec_for<'s>(specs: &'s [Command], name: &str, cwd: &Path) -> Option<&'s Command> {
    specs.iter().find(|s| s.name == name).filter(|s| {
        s.requires_files.is_empty() || cwd.ancestors().any(|d| s.requires_files.iter().any(|f| d.join(f).exists()))
    })
}

/// Values from the default Docker or kubectl context would be wrong for a command that names another host,
/// context, config or (kubectl) namespace, so providers are not asked for it.
fn targets_elsewhere(command: &str, words: &[String]) -> bool {
    let (long, short): (&[&str], &[&str]) = match command {
        "docker" => (&["--context", "--host", "--config"], &["-H", "-c"]),
        "kubectl" => (&["--context", "--kubeconfig", "--cluster", "--server", "--namespace", "--all-namespaces"], &["-n", "-s", "-A"]),
        _ => return false,
    };
    words.iter().any(|w| {
        long.iter().any(|o| w.strip_prefix(o).is_some_and(|rest| rest.is_empty() || rest.starts_with('=')))
            || short.iter().any(|o| w.starts_with(o))
    })
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

/// The letter of a single-letter option (`-x`).
fn short_letter(name: &str) -> Option<char> {
    let mut chars = name.strip_prefix('-')?.chars();
    chars.next().filter(|c| c.is_ascii_alphanumeric() && chars.next().is_none())
}

/// `-xzf` read as `-x -z -f`: every letter one of `node`'s single-letter options, and only the last may take a value.
fn short_bundle<'n>(node: &'n Command, word: &str) -> Option<Vec<&'n Opt>> {
    let letters = word.strip_prefix('-').filter(|l| l.len() >= 2 && !l.starts_with('-'))?;
    let opts: Vec<&Opt> = letters.chars().map(|c| node.options.iter().find(|o| short_letter(&o.name) == Some(c))).collect::<Option<_>>()?;
    opts[..opts.len() - 1].iter().all(|o| !o.takes_value).then_some(opts)
}

/// What the whole line shares: where it is typed, the host's data and what the shell knows.
#[derive(Clone, Copy)]
struct Env<'a> {
    cwd: &'a Path,
    values: &'a dyn Fn(Provider, &Path) -> Vec<Suggestion>,
    recall: Option<&'a RecallFn>,
    specs: &'a [Command],
    shell: &'a ShellContext,
    /// The word under the cursor is written as it reads: no quotes or escapes.
    plain: bool,
    quoted: bool,
}

/// Collects matching candidates for the word under the cursor.
struct Sink<'a> {
    typed: String,
    start: usize,
    end: usize,
    env: Env<'a>,
    providers_enabled: bool,
    /// Folders also come from `$CDPATH` (`cd`).
    cdpath: bool,
    out: Vec<Candidate>,
    /// Words one slip away from what was typed; shown only when nothing else matched.
    near: Vec<Candidate>,
}

impl<'a> Sink<'a> {
    fn new(typed: String, start: usize, end: usize, env: Env<'a>) -> Self {
        Self { typed, start, end, env, providers_enabled: true, cdpath: false, out: Vec::new(), near: Vec::new() }
    }

    /// Complete only the part after `--opt=`.
    fn skip_to_value(&mut self, skip: usize) {
        self.typed.drain(..skip);
        self.start += skip;
    }

    fn push(&mut self, label: &str, description: Option<&str>, kind: Kind, insert_text: String, source: Source, scope: &str) {
        let found = if kind == Kind::Directory { matching::match_path(&self.typed, label) } else { matching::match_label(&self.typed, label) };
        let (m, close) = match found {
            Some(m) => (m, false),
            // Typing `chekcout` finds `checkout`, but a file name is not a word to correct.
            None if !matches!(kind, Kind::File | Kind::Directory) => match matching::near(&self.typed, label) {
                Some(m) => (m, true),
                None => return,
            },
            None => return,
        };
        let candidate = self.candidate(label, description, kind, insert_text, source, scope, m);
        if close { self.near.push(candidate) } else { self.out.push(candidate) }
    }

    #[allow(clippy::too_many_arguments)]
    fn candidate(&self, label: &str, description: Option<&str>, kind: Kind, insert_text: String, source: Source, scope: &str, m: matching::Match) -> Candidate {
        Candidate {
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
            cursor_offset: None,
            takes_value: false,
            used: false,
        }
    }

    /// The command word: commands with a spec and, once something is typed, whatever the shell can run.
    fn command_names(&mut self) {
        let (specs, shell, plain) = (self.env.specs, self.env.shell, self.env.plain);
        for s in specs {
            self.push(&s.name, s.description.as_deref(), Kind::Command, format!("{} ", s.name), Source::Bundled, &s.name);
        }
        if plain && self.typed.contains('/') {
            self.files(false, &[]);
        } else if plain && !self.typed.is_empty() {
            // Not every command has a spec: offer what the shell itself can run.
            for (name, kind) in &shell.names {
                let (desc, scope) = match kind {
                    NameKind::Alias => (shell.aliases.get(name).map_or("alias".to_string(), |v| format!("alias: {v}")), "alias"),
                    NameKind::Function => ("shell function".to_string(), "function"),
                    NameKind::Builtin => ("shell builtin".to_string(), "builtin"),
                    NameKind::Executable => ("command".to_string(), "path"),
                };
                self.push(name, Some(&desc), Kind::Command, format!("{} ", fsys::escape(name)), Source::Generator, scope);
            }
        }
    }

    /// Values used before for this option of this command, ahead of everything else the slot offers.
    fn recalled(&mut self, command: &str, option: &str) {
        let Some(recall) = self.env.recall else { return };
        for value in recall(command, option, self.env.cwd) {
            let insert = format!("{} ", fsys::escape(&value));
            self.push(&value, Some("used before"), Kind::Argument, insert, Source::Local, command);
        }
    }

    /// Names of the shell's exported variables. `skip` bytes of the word (the `$` or `${`) stay as typed.
    fn env_names(&mut self, skip: usize, tail: &str) {
        self.skip_to_value(skip);
        let shell = self.env.shell;
        for name in shell.env.iter().filter(|n| is_identifier(n)) {
            self.push(name, Some("environment variable"), Kind::Argument, format!("{name}{tail}"), Source::Local, "env");
        }
    }

    /// The user's own names for subcommands, as subcommands.
    fn aliases(&mut self, provider: Provider, scope: &str) {
        for a in (self.env.values)(provider, self.env.cwd) {
            let description = a.description.as_deref().map(|d| format!("alias: {d}"));
            self.push(&a.name, description.as_deref(), Kind::Subcommand, format!("{} ", fsys::escape(&a.name)), Source::Generator, scope);
        }
    }

    /// The words a subcommand alias stands for.
    fn alias(&self, provider: Provider, word: &str) -> Option<Vec<String>> {
        let value = (self.env.values)(provider, self.env.cwd).into_iter().find(|a| a.name == word)?.description?;
        let words: Vec<String> = value.split_whitespace().map(str::to_string).collect();
        (!words.is_empty()).then_some(words)
    }

    /// Options given already are left out, unless they can repeat or take a value (`-e A=1 -e B=2`), or are
    /// exactly what is being typed.
    fn options(&mut self, node: &Command, used: &HashSet<&str>, scope: &str) {
        for o in &node.options {
            if used.contains(o.name.as_str()) && !o.is_repeatable && !o.takes_value && self.typed != o.name {
                continue;
            }
            let (out, near) = (self.out.len(), self.near.len());
            self.push(&o.name, o.description.as_deref(), Kind::Option, format!("{} ", o.name), Source::Bundled, scope);
            for c in self.out[out..].iter_mut().chain(&mut self.near[near..]) {
                c.takes_value = o.takes_value;
            }
        }
    }

    /// `tar -xz`: the single-letter options that can still join the group, each inserted as part of it.
    fn bundle(&mut self, node: &Command, bundle: &[&Opt], used: &HashSet<&str>, scope: &str) {
        for o in &node.options {
            let Some(letter) = short_letter(&o.name) else { continue };
            if bundle.iter().any(|b| b.name == o.name) || (used.contains(o.name.as_str()) && !o.is_repeatable && !o.takes_value) {
                continue;
            }
            let m = matching::Match { score: 100.0, ranges: vec![] };
            let mut c = self.candidate(&o.name, o.description.as_deref(), Kind::Option, format!("{}{letter} ", self.typed), Source::Bundled, scope, m);
            c.takes_value = o.takes_value;
            self.out.push(c);
        }
    }

    /// Values of the slot that start with a dash (`-` for the previous branch, `-globalDomain`).
    fn dash_values(&mut self, arg: Option<&Arg>, scope: &str) {
        for s in arg.into_iter().flat_map(|a| &a.suggestions).filter(|s| s.name.starts_with('-')) {
            let insert = s.insert_value.clone().unwrap_or_else(|| fsys::escape(&s.name));
            self.push(&s.name, s.description.as_deref(), Kind::Argument, format!("{insert} "), Source::Bundled, scope);
        }
    }

    fn arg_values(&mut self, arg: Option<&Arg>, scope: &str) {
        let Some(arg) = arg else { return };
        // A wrapper's own project comes first (`npx` lists `node_modules/.bin` before its well-known packages).
        if arg.runs_command {
            self.provided(arg, scope);
        }
        // Values written as a dash look like options: they come with the options (`dash_values`).
        for s in arg.suggestions.iter().filter(|s| !s.name.starts_with('-')) {
            let insert = s.insert_value.clone().unwrap_or_else(|| fsys::escape(&s.name));
            self.push(&s.name, s.description.as_deref(), Kind::Argument, format!("{insert} "), Source::Bundled, scope);
        }
        if !arg.runs_command {
            self.provided(arg, scope);
        }
        match arg.template {
            Some(Template::Filepaths) => self.files(false, &arg.extensions),
            Some(Template::Folders) => self.files(true, &[]),
            Some(Template::Commands) => self.command_names(),
            None => {}
        }
        // The command a wrapper runs, when the spec names no particular ones (`sudo`, `time`; not `npx`'s programs).
        let names_none = arg.suggestions.is_empty() && arg.provider.is_none() && arg.providers.is_empty() && arg.template.is_none();
        if arg.runs_command && names_none {
            self.command_names();
        }
    }

    /// What the slot's providers give.
    fn provided(&mut self, arg: &Arg, scope: &str) {
        // Frequently used directories only narrow what is typed; listing them all for an empty word would bury the cwd.
        let (enabled, typed_empty) = (self.providers_enabled, self.typed.is_empty());
        let wanted = |p: Provider| enabled && (p != Provider::FrecentDirs || !typed_empty);
        let providers: Vec<Provider> = arg.provider.iter().chain(&arg.providers).copied().filter(|&p| wanted(p)).collect();
        for provider in providers {
            let kind = if provider == Provider::FrecentDirs { Kind::Directory } else { Kind::Argument };
            for value in (self.env.values)(provider, self.env.cwd) {
                // A provider may say what to insert (`scp` wants `host:`, with no space after it).
                let insert = value.insert_value.clone().unwrap_or_else(|| format!("{} ", fsys::escape(&value.name)));
                self.push(&value.name, value.description.as_deref(), kind, insert, Source::Generator, scope);
            }
        }
    }

    /// `extensions` (without the dot) keep only files that end in one of them; directories always stay.
    fn files(&mut self, dirs_only: bool, extensions: &[String]) {
        let listing = fsys::list(self.env.cwd, &self.typed, dirs_only);
        // Match against the basename only; the directory part is kept verbatim.
        let full_typed = std::mem::replace(&mut self.typed, listing.typed_name.clone());
        self.listed(&listing, extensions, None);
        self.typed = full_typed;
        // `cd name` also finds `name` in the folders of `$CDPATH`, as `cd` itself does.
        if dirs_only && self.cdpath && !self.typed.starts_with(['/', '.', '~']) {
            let shell = self.env.shell;
            for base in &shell.cdpath {
                let listing = fsys::list(Path::new(base), &self.typed, true);
                let full_typed = std::mem::replace(&mut self.typed, listing.typed_name.clone());
                self.listed(&listing, &[], Some(&format!("in {base}")));
                self.typed = full_typed;
            }
        }
    }

    fn listed(&mut self, listing: &fsys::Listing, extensions: &[String], description: Option<&str>) {
        let wanted = |name: &str| {
            let lower = name.to_lowercase();
            extensions.is_empty() || extensions.iter().any(|x| lower.ends_with(&format!(".{}", x.to_lowercase())))
        };
        for e in listing.entries.iter().filter(|e| e.is_dir || wanted(&e.name)) {
            let label = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
            let (kind, tail) = if e.is_dir { (Kind::Directory, "") } else { (Kind::File, " ") };
            let insert = format!("{}{tail}", fsys::escape(&format!("{}{label}", listing.dir_prefix)));
            self.push(&label, description, kind, insert, Source::Filesystem, "fs");
        }
    }

    fn finish(mut self) -> Vec<Candidate> {
        if self.out.is_empty() {
            self.out = std::mem::take(&mut self.near);
        }
        let sort_by_label = !self.typed.is_empty();
        // Stable: with nothing typed, spec order is kept.
        self.out.sort_by(|a, b| {
            b.score.total_cmp(&a.score).then_with(|| if sort_by_label { a.label.cmp(&b.label) } else { std::cmp::Ordering::Equal })
        });
        let mut seen = std::collections::HashSet::new();
        // A provider's file and the same file from the directory listing are one suggestion (the first, better placed, stays).
        self.out.retain(|c| seen.insert((c.insert_text.clone(), c.replace_start_byte, c.replace_end_byte)));
        self.out.truncate(MAX_RESULTS);
        self.out
    }
}
