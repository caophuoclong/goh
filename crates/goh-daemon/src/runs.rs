//! Opt-in record of the command lines you ran (`privacy.recordCommands`, off unless you turn it on), used for
//! three things: finishing a line you have started from one you ran before, guessing the command that
//! usually follows the last one in this folder, and remembering the values you gave an option.
//!
//! The shell's own history (`~/.zsh_history`) counts too while recording is on, for what was run before goh kept its
//! record: the same filters apply, and its lines are never "this folder" (it does not say where they ran).
//!
//! One appended line per successful command: `epoch TAB cwd TAB command`. The file is private to you (0600), local,
//! bounded, and lines that look like they carry a secret are never written (see [`looks_secret`]), nor are lines that
//! start with a space (the usual way to say "keep this out of my history"). `goh forget` deletes it.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use goh_core::protocol::{Candidate, Kind, Source};

use crate::usage::weight;

const COMPACT_ABOVE: u64 = 1 << 20;
const KEEP_LINES: usize = 4000;
const MAX_LINE: usize = 200;
/// Two commands further apart than this in one folder are not "one after the other".
const SEQUENCE_GAP: u64 = 6 * 3600;
/// A prediction is for the prompt right after a command, not for one from this morning.
const PREDICT_WITHIN: u64 = 30 * 60;
const TTL: Duration = Duration::from_secs(2);
const MIN_REPEATS: usize = 2;
/// The newest commands kept from the shell's history.
const SHELL_HISTORY_LINES: usize = 10_000;

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("GOH_RUNS") {
        return p.into();
    }
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    };
    base.join("goh/runs")
}

/// The zsh history file: `$GOH_SHELL_HISTORY`, `$HISTFILE` when exported, else `.zsh_history` in `$ZDOTDIR` or home.
pub fn shell_history_path() -> Option<PathBuf> {
    for var in ["GOH_SHELL_HISTORY", "HISTFILE"] {
        if let Some(p) = std::env::var_os(var).filter(|p| !p.is_empty()) {
            return Some(p.into());
        }
    }
    let dir = std::env::var_os("ZDOTDIR").filter(|d| !d.is_empty()).or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(dir).join(".zsh_history"))
}

/// zsh writes bytes it treats specially as 0x83 followed by the byte XOR 0x20 ("metafied"), non-ASCII text included.
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut it = bytes.iter();
    while let Some(&b) = it.next() {
        out.push(if b == 0x83 { it.next().map_or(b, |n| n ^ 0x20) } else { b });
    }
    out
}

/// Commands from a zsh history file, oldest first, with no folder. Extended lines (`: epoch:secs;command`) keep their
/// time; plain ones are spread over the seconds before `modified`, in order. A command over several lines is left
/// out, and so is every line [`accept`] would not keep.
fn parse_shell_history(bytes: &[u8], modified: u64) -> Vec<Run> {
    let text = String::from_utf8_lossy(&unmetafy(bytes)).into_owned();
    let mut entries: Vec<(Option<u64>, String)> = Vec::new();
    let mut continued = false;
    for line in text.lines() {
        let was_continued = std::mem::replace(&mut continued, line.ends_with('\\'));
        if was_continued || continued {
            continue;
        }
        let (at, command) = match line.strip_prefix(": ").and_then(|r| r.split_once(';')) {
            Some((stamp, command)) => (stamp.split(':').next().and_then(|t| t.parse().ok()), command),
            None => (None, line),
        };
        if let Some(command) = accept(command) {
            entries.push((at, command));
        }
    }
    let skip = entries.len().saturating_sub(SHELL_HISTORY_LINES);
    let n = entries.len() as u64;
    entries
        .into_iter()
        .enumerate()
        .skip(skip)
        .map(|(i, (at, line))| Run { at: at.unwrap_or_else(|| modified.saturating_sub(n - i as u64)), cwd: String::new(), line })
        .collect()
}

/// `GOH_RECORD` (`on` / `off`) if set, else `privacy.recordCommands` in the config file: off unless it says `true`.
pub fn enabled() -> bool {
    match std::env::var("GOH_RECORD").as_deref() {
        Ok("on" | "1" | "true") => true,
        Ok("off" | "0" | "false") => false,
        _ => goh_theme::read_config().and_then(|c| c.pointer("/privacy/recordCommands").and_then(|v| v.as_bool())).unwrap_or(false),
    }
}

const SECRET_WORDS: [&str; 15] = [
    "password", "passwd", "passphrase", "secret", "token", "apikey", "api_key", "api-key", "authorization", "bearer ",
    "private_key", "credential", "-----begin", "access_key", "cookie",
];

/// A long run of letters and digits with no path or option look to it: a key, a token, a hash.
fn opaque(word: &str) -> bool {
    let value = word.rsplit('=').next().unwrap_or(word).trim_matches(|c| c == '"' || c == '\'');
    value.len() >= 24
        && !value.starts_with(['/', '.', '~', '-'])
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '_' | '-' | '=' | '.'))
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_alphabetic())
}

/// Whether a line probably carries a secret: a word like `password` or `token`, a `user:pass@host` URL, or an
/// opaque token. It errs on the side of leaving a line out; a missed suggestion costs less than a kept secret.
pub fn looks_secret(line: &str) -> bool {
    let lower = line.to_lowercase();
    if SECRET_WORDS.iter().any(|w| lower.contains(w)) {
        return true;
    }
    if let Some(i) = line.find("://") {
        let authority = line[i + 3..].split(['/', ' ']).next().unwrap_or("");
        if authority.split_once('@').is_some_and(|(user, _)| user.contains(':')) {
            return true;
        }
    }
    line.split_whitespace().any(opaque)
}

/// The line as it will be kept, or `None` when it must not be: empty, multi-line, long, secret-looking, started
/// with a space, or one of goh's own.
pub fn accept(command: &str) -> Option<String> {
    if command.starts_with(' ') {
        return None;
    }
    let line = command.trim();
    if line.chars().count() < 2 || line.len() > MAX_LINE || line.chars().any(char::is_control) || looks_secret(line) {
        return None;
    }
    (line.split_whitespace().next()? != "goh").then(|| line.to_string())
}

pub fn record(file: &Path, cwd: &str, command: &str, at: u64) -> std::io::Result<()> {
    let Some(line) = accept(command) else { return Ok(()) };
    if !cwd.starts_with('/') || cwd.len() > 1024 || cwd.chars().any(char::is_control) {
        return Ok(());
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).mode(0o600).open(file)?.write_all(format!("{at}\t{cwd}\t{line}\n").as_bytes())?;
    if fs::metadata(file).is_ok_and(|m| m.len() > COMPACT_ABOVE) {
        let text = fs::read_to_string(file)?;
        let lines: Vec<&str> = text.lines().collect();
        let tmp = file.with_extension("tmp");
        OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&tmp)?
            .write_all((lines[lines.len().saturating_sub(KEEP_LINES)..].join("\n") + "\n").as_bytes())?;
        fs::rename(tmp, file)?;
    }
    Ok(())
}

/// Delete the record. `Ok(false)`: there was none.
pub fn forget(file: &Path) -> std::io::Result<bool> {
    match fs::remove_file(file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// The program each recorded line starts with and how often it was run, most-run first (ties by name). A leading
/// `VAR=value` assignment is skipped; a word that is not a plain program name (a path, a subshell) is left out.
pub fn programs(record: &str) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in record.lines() {
        let Some(command) = line.splitn(3, '\t').nth(2) else { continue };
        let Some(word) = command.split_whitespace().find(|w| !w.contains('=')) else { continue };
        if word.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+')) && !word.starts_with(['-', '.']) {
            *counts.entry(word).or_default() += 1;
        }
    }
    let mut all: Vec<(String, usize)> = counts.into_iter().map(|(w, n)| (w.to_string(), n)).collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    all
}

/// `git add .` -> `git add`; `cd src` -> `cd`; `cargo test -p x` -> `cargo test`: the part of a line that says what
/// kind of command it is, so that "what follows `git add`" does not depend on its arguments.
pub fn command_key(line: &str) -> String {
    const TAKES_A_PATH: [&str; 16] = ["cd", "ls", "cat", "vim", "nvim", "code", "open", "less", "rm", "mv", "cp", "mkdir", "touch", "echo", "tail", "head"];
    let mut words = line.split_whitespace();
    let Some(first) = words.next() else { return String::new() };
    let second = words.next().filter(|w| {
        !TAKES_A_PATH.contains(&first) && w.len() <= 24 && w.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | ':'))
    });
    match second {
        Some(s) => format!("{first} {s}"),
        None => first.to_string(),
    }
}

/// Split a line into words the way a shell would for plain cases: quotes group, backslash escapes.
fn words_of(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut escaped, mut started) = (Vec::new(), String::new(), None::<char>, false, false);
    for c in line.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if let Some(q) = quote {
            if c == q { quote = None } else { cur.push(c) }
        } else if c == '\\' {
            escaped = true;
            started = true;
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() {
            if started {
                out.push(std::mem::take(&mut cur));
                started = false;
            }
        } else {
            cur.push(c);
            started = true;
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// Commands that do their job once for what they name: install or remove a package, create a branch, a tag, a
/// clone. Each is the words it starts with; it is done once only when it names a target after them.
const DONE_ONCE: &[&[&str]] = &[
    &["brew", "install"], &["brew", "reinstall"], &["brew", "uninstall"], &["brew", "remove"], &["brew", "rm"], &["brew", "tap"], &["brew", "untap"],
    &["npm", "install"], &["npm", "i"], &["npm", "add"], &["npm", "uninstall"], &["npm", "remove"], &["npm", "rm"], &["npm", "un"], &["npm", "create"], &["npm", "init"],
    &["pnpm", "add"], &["pnpm", "install"], &["pnpm", "i"], &["pnpm", "remove"], &["pnpm", "rm"], &["pnpm", "create"],
    &["yarn", "add"], &["yarn", "remove"], &["yarn", "create"], &["yarn", "global", "add"], &["yarn", "global", "remove"],
    &["bun", "add"], &["bun", "install"], &["bun", "i"], &["bun", "remove"], &["bun", "rm"], &["bun", "create"],
    &["pip", "install"], &["pip", "uninstall"], &["pip3", "install"], &["pip3", "uninstall"], &["pipx", "install"], &["pipx", "uninstall"],
    &["uv", "add"], &["uv", "remove"], &["uv", "pip", "install"], &["uv", "pip", "uninstall"], &["uv", "tool", "install"], &["uv", "tool", "uninstall"],
    &["cargo", "install"], &["cargo", "uninstall"], &["cargo", "add"], &["cargo", "remove"], &["cargo", "new"], &["cargo", "init"],
    &["gem", "install"], &["gem", "uninstall"], &["go", "install"], &["go", "get"],
    &["apt", "install"], &["apt", "remove"], &["apt", "purge"], &["apt-get", "install"], &["apt-get", "remove"], &["apt-get", "purge"],
    &["dnf", "install"], &["dnf", "remove"], &["yum", "install"], &["yum", "remove"], &["apk", "add"], &["apk", "del"],
    &["port", "install"], &["port", "uninstall"], &["mas", "install"], &["nvm", "install"], &["nvm", "uninstall"],
    &["pyenv", "install"], &["pyenv", "uninstall"], &["asdf", "install"], &["asdf", "plugin", "add"],
    &["rustup", "toolchain", "install"], &["rustup", "target", "add"], &["rustup", "component", "add"],
    &["git", "clone"], &["git", "checkout", "-b"], &["git", "checkout", "-B"], &["git", "switch", "-c"], &["git", "switch", "-C"],
    &["git", "branch"], &["git", "tag"], &["git", "remote", "add"], &["git", "remote", "remove"], &["git", "remote", "rm"], &["git", "worktree", "add"],
    &["docker", "volume", "create"], &["docker", "network", "create"], &["kubectl", "create"],
];

/// A line that did its job the one time it ran (`brew install jq`, `git checkout -b fix`): running it again does
/// nothing or fails, so it is not offered again. What is reinstalled from here (`pip install -r requirements.txt`,
/// `cargo install --path .`, a bare `npm install`) is repeatable, and so is any command not in [`DONE_ONCE`].
pub fn done_once(line: &str) -> bool {
    let words = words_of(line);
    let words: Vec<&str> = words.iter().map(String::as_str).skip_while(|w| w.contains('=') || *w == "sudo").collect();
    let Some(prefix) = DONE_ONCE.iter().filter(|p| words.starts_with(p)).max_by_key(|p| p.len()) else { return false };
    let rest = &words[prefix.len()..];
    let local = |w: &&str| w.starts_with(['.', '/', '~']) || matches!(*w, "-r" | "--requirement" | "-e" | "--editable" | "--path");
    rest.iter().any(|w| !w.starts_with('-')) && !rest.iter().any(local)
}

fn secret_name(option: &str) -> bool {
    let lower = option.to_lowercase();
    ["token", "pass", "secret", "key", "auth", "cred", "cookie"].iter().any(|w| lower.contains(w))
}

/// How many times, and with what recency weight.
type Tally = (usize, f64);

#[derive(Debug, Clone)]
struct Run {
    at: u64,
    cwd: String,
    line: String,
}

/// A line seen before, with how much it has been used.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub line: String,
    pub count: usize,
    /// Run in this folder at least once.
    pub here: bool,
    pub score: f64,
}

#[derive(Debug, Default)]
pub struct Runs {
    runs: Vec<Run>,
}

impl Runs {
    pub fn load(file: &Path) -> Runs {
        let Ok(text) = fs::read_to_string(file) else { return Runs::default() };
        let mut runs: Vec<Run> = text
            .lines()
            .filter_map(|l| {
                let mut f = l.splitn(3, '\t');
                Some(Run { at: f.next()?.parse().ok()?, cwd: f.next()?.to_string(), line: f.next()?.to_string() })
            })
            .collect();
        let skip = runs.len().saturating_sub(KEEP_LINES);
        runs.drain(..skip);
        Runs { runs }
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Put the shell's history before goh's own record, keeping only what is older than the record's first line:
    /// from then on goh has the same commands, with their folders.
    fn with_shell_history(mut self, shell: &[Run]) -> Runs {
        let since = self.runs.first().map_or(u64::MAX, |r| r.at);
        let mut all: Vec<Run> = shell.iter().filter(|r| r.at < since).cloned().collect();
        all.append(&mut self.runs);
        Runs { runs: all }
    }

    /// Lines that begin with `typed` and go on, best first: used more, more recently, and in this folder. Lines
    /// that did their job once ([`done_once`]) are left out.
    pub fn lines_starting(&self, typed: &str, cwd: &str, now: u64, limit: usize) -> Vec<Seen> {
        if typed.chars().count() < 3 {
            return vec![];
        }
        let mut found: HashMap<&str, Seen> = HashMap::new();
        for r in self.runs.iter().filter(|r| r.line.len() > typed.len() && r.line.starts_with(typed) && !done_once(&r.line)) {
            let here = r.cwd == cwd;
            let w = weight(now.saturating_sub(r.at));
            let seen = found.entry(&r.line).or_insert_with(|| Seen { line: r.line.clone(), count: 0, here: false, score: 0.0 });
            seen.count += 1;
            seen.here |= here;
            seen.score += if here { 2.0 * w } else { w };
        }
        let mut out: Vec<Seen> = found.into_values().collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.line.cmp(&b.line)));
        out.truncate(limit);
        out
    }

    /// What has usually followed the last command run in this folder, if that was moments ago. Only what was
    /// repeated (`MIN_REPEATS` times) counts: a full line when the same one followed, else just the command
    /// (`git commit `) when only that was repeated.
    pub fn next_after(&self, cwd: &str, now: u64, limit: usize) -> Vec<Seen> {
        let Some(prev) = self.runs.iter().rev().find(|r| r.cwd == cwd) else { return vec![] };
        if now.saturating_sub(prev.at) > PREDICT_WITHIN {
            return vec![];
        }
        let key = command_key(&prev.line);
        let mut by_line: HashMap<&str, Tally> = HashMap::new();
        let mut by_key: HashMap<String, Tally> = HashMap::new();
        let mut last_in_folder: HashMap<&str, &Run> = HashMap::new();
        // The shell's history says nothing about folders, so it says nothing about what follows what here.
        for r in self.runs.iter().filter(|r| !r.cwd.is_empty()) {
            if last_in_folder.get(r.cwd.as_str()).is_some_and(|p| r.at.saturating_sub(p.at) <= SEQUENCE_GAP && command_key(&p.line) == key) {
                let w = weight(now.saturating_sub(r.at));
                let line = by_line.entry(&r.line).or_default();
                (line.0, line.1) = (line.0 + 1, line.1 + w);
                let kind = by_key.entry(command_key(&r.line)).or_default();
                (kind.0, kind.1) = (kind.0 + 1, kind.1 + w);
            }
            last_in_folder.insert(&r.cwd, r);
        }
        let mut lines: Vec<Seen> = by_line
            .into_iter()
            .filter(|(l, (n, _))| *n >= MIN_REPEATS && !done_once(l))
            .map(|(l, (count, score))| Seen { line: l.to_string(), count, here: true, score })
            .collect();
        lines.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.line.cmp(&b.line)));
        let mut keys: Vec<(String, Tally)> = by_key.into_iter().filter(|(k, (n, _))| *n >= MIN_REPEATS && !k.is_empty()).collect();
        keys.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1).then_with(|| a.0.cmp(&b.0)));
        for (k, (count, score)) in keys {
            // `git commit ` adds nothing when full `git commit -m ...` lines already stand for it.
            if !lines.iter().any(|l| command_key(&l.line) == k) {
                lines.push(Seen { line: format!("{k} "), count, here: true, score });
            }
        }
        lines.truncate(limit);
        lines
    }

    /// Values given after `option` (`-n staging`, `--region=eu-west-1`) in earlier runs of `command`, best first.
    /// Options whose names suggest a secret, and values that look like one, are never returned.
    pub fn values_for(&self, command: &str, option: &str, cwd: &str, now: u64) -> Vec<String> {
        if secret_name(option) || option.len() > 40 {
            return vec![];
        }
        let inline = format!("{option}=");
        let mut found: HashMap<String, f64> = HashMap::new();
        for r in self.runs.iter().rev().take(2000) {
            let words = words_of(&r.line);
            if words.first().map(String::as_str) != Some(command) {
                continue;
            }
            for (i, w) in words.iter().enumerate().skip(1) {
                let value = if w == option { words.get(i + 1) } else { None }.map(String::as_str).or_else(|| w.strip_prefix(&inline));
                let Some(v) = value.filter(|v| !v.is_empty() && v.len() <= 80 && !v.starts_with('-') && !v.chars().any(char::is_control) && !opaque(v) && !looks_secret(v)) else { continue };
                let w = weight(now.saturating_sub(r.at));
                *found.entry(v.to_string()).or_default() += if r.cwd == cwd { 2.0 * w } else { w };
            }
        }
        let mut out: Vec<(String, f64)> = found.into_iter().collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out.into_iter().take(8).map(|(v, _)| v).collect()
    }
}

fn candidate(seen: &Seen, start: usize, end: usize, typed_len: usize, note: String) -> Candidate {
    Candidate {
        id: format!("history:{}", seen.line),
        label: seen.line.clone(),
        description: Some(note),
        kind: Kind::History,
        insert_text: seen.line.clone(),
        replace_start_byte: start,
        replace_end_byte: end,
        match_ranges: if typed_len > 0 { vec![[0, typed_len]] } else { vec![] },
        // Below every suggestion from a spec: these come after them in the menu.
        score: 20.0 + seen.score.min(15.0),
        source: Source::Local,
        cursor_offset: None,
        takes_value: false,
        used: false,
    }
}

/// Whole lines for the menu: at an empty prompt, what usually comes next here; mid-line at its end, the lines you
/// ran before that start with what is typed. At most three; nothing when the record is empty or off.
pub fn candidates(runs: &Runs, buffer: &str, cursor: usize, cwd: &str, now: u64) -> Vec<Candidate> {
    if runs.is_empty() || cursor != buffer.len() {
        return vec![];
    }
    let times = |n: usize| if n > 1 { format!("{n}×") } else { "once".to_string() };
    if buffer.is_empty() {
        return runs
            .next_after(cwd, now, 3)
            .iter()
            .map(|s| candidate(s, 0, 0, 0, format!("usually next here · {}", times(s.count))))
            .collect();
    }
    runs.lines_starting(buffer, cwd, now, 3)
        .iter()
        .map(|s| candidate(s, 0, buffer.len(), buffer.len(), format!("history · {}{}", times(s.count), if s.here { " · this folder" } else { "" })))
        .collect()
}

/// The shell's history as last parsed, and the file's size and time then: parsed again only when they change.
type ShellHistory = Option<((PathBuf, u64, u64), Arc<Vec<Run>>)>;

/// The record (with the shell's history before it), re-read at most every couple of seconds, and empty while
/// recording is off.
pub struct RunsCache {
    file: PathBuf,
    shell_history: Option<PathBuf>,
    /// Fixed on or off, ignoring the setting (tests); `None` asks [`enabled`] each time the file is re-read.
    forced: Option<bool>,
    inner: Mutex<(Instant, Arc<Runs>)>,
    shell: Mutex<ShellHistory>,
}

impl RunsCache {
    pub fn new(file: PathBuf) -> Self {
        Self { file, shell_history: shell_history_path(), forced: None, inner: Mutex::new((Instant::now() - TTL, Arc::default())), shell: Mutex::new(None) }
    }

    /// Fixed on or off, reading `shell_history` (if any) instead of the user's own.
    pub fn forced(file: PathBuf, on: bool, shell_history: Option<PathBuf>) -> Self {
        Self { forced: Some(on), shell_history, ..Self::new(file) }
    }

    fn shell_runs(&self) -> Arc<Vec<Run>> {
        let Some(path) = &self.shell_history else { return Arc::default() };
        let Ok(meta) = fs::metadata(path) else { return Arc::default() };
        let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let stamp = (path.clone(), meta.len(), modified);
        let Ok(mut cached) = self.shell.lock() else { return Arc::default() };
        if let Some((_, runs)) = cached.as_ref().filter(|(s, _)| *s == stamp) {
            return Arc::clone(runs);
        }
        let runs = Arc::new(fs::read(path).map(|b| parse_shell_history(&b, modified)).unwrap_or_default());
        *cached = Some((stamp, Arc::clone(&runs)));
        runs
    }

    pub fn get(&self) -> Arc<Runs> {
        let Ok(mut cached) = self.inner.lock() else { return Arc::default() };
        if cached.0.elapsed() >= TTL {
            let loaded = if self.forced.unwrap_or_else(enabled) { Runs::load(&self.file).with_shell_history(&self.shell_runs()) } else { Runs::default() };
            *cached = (Instant::now(), Arc::new(loaded));
        }
        Arc::clone(&cached.1)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn programs_are_counted_from_the_first_word_of_each_line() {
        let record = "1\t/a\tgit status\n2\t/a\tFOO=1 jq . x\n3\t/a\tgit add .\n4\t/a\t./run.sh\n5\t/a\tjq -r .a\n6\t/a\tzz\nbroken line\n";
        assert_eq!(programs(record), vec![("git".into(), 2), ("jq".into(), 2), ("zz".into(), 1)]);
    }

    use super::*;

    const NOW: u64 = 10_000_000;

    fn runs(lines: &[(u64, &str, &str)]) -> Runs {
        Runs { runs: lines.iter().map(|(at, cwd, line)| Run { at: *at, cwd: cwd.to_string(), line: line.to_string() }).collect() }
    }

    #[test]
    fn secrets_and_private_lines_are_never_kept() {
        for bad in [
            "curl -H 'Authorization: Bearer abc' https://x",
            "export API_KEY=abc",
            "mysql --password=hunter2",
            "git clone https://me:pw@host/repo.git",
            "aws s3 ls --token abcdefghijklmnopqrstuvwxyz0123456789",
            "deploy ghp1234567890abcdefghijklmnopqrstuvwx",
            " ls -la",
            "x",
            "goh forget",
            "echo a\nb",
        ] {
            assert_eq!(accept(bad), None, "{bad:?}");
        }
        for fine in ["git status", "cargo test -p goh-core", "cd ~/Code/some-long-project-name-here", "docker run -p 80:80 nginx:1.25", "git clone https://github.com/a/b.git"] {
            assert_eq!(accept(fine).as_deref(), Some(fine), "{fine:?}");
        }
    }

    #[test]
    fn the_record_is_private_bounded_and_forgettable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data/runs");
        record(&file, "/work", "git status", NOW).unwrap();
        record(&file, "/work", "export TOKEN=abc", NOW).unwrap();
        record(&file, "relative", "ls -la", NOW).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), format!("{NOW}\t/work\tgit status\n"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(forget(&file).unwrap() && !forget(&file).unwrap());
    }

    #[test]
    fn a_started_line_is_finished_from_what_was_run_most_and_most_recently_here() {
        let r = runs(&[
            (NOW - 9_000_000, "/a", "docker compose up -d"),
            (NOW - 100, "/b", "docker compose logs -f web"),
            (NOW - 50, "/b", "docker compose logs -f web"),
            (NOW - 40, "/a", "docker compose up -d --build"),
            (NOW - 30, "/b", "dock"),
        ]);
        let found = r.lines_starting("docker compose ", "/b", NOW, 3);
        assert_eq!(found.iter().map(|s| s.line.as_str()).collect::<Vec<_>>(), ["docker compose logs -f web", "docker compose up -d --build", "docker compose up -d"]);
        assert_eq!((found[0].count, found[0].here, found[1].here), (2, true, false));
        assert!(r.lines_starting("do", "/b", NOW, 3).is_empty(), "two characters are too few to guess from");
        assert!(r.lines_starting("docker compose up -d --build", "/b", NOW, 3).is_empty(), "a finished line adds nothing");
    }

    #[test]
    fn the_command_that_usually_follows_is_predicted_only_after_it_repeated_and_only_just_after() {
        let messages = ["git commit -m a", "git commit -m b", "git commit -m c"];
        let mut lines = Vec::new();
        for (i, message) in messages.iter().enumerate() {
            let t = NOW - 100_000 + i as u64 * 1000;
            lines.extend([(t, "/p", "git add ."), (t + 5, "/p", *message), (t + 9, "/p", "git push")]);
        }
        lines.push((NOW - 60, "/p", "git add -A"));
        let r = runs(&lines);
        let next = r.next_after("/p", NOW, 3);
        // The last command was `git add`; `git commit` followed it each time, with a different message each time.
        assert_eq!(next.iter().map(|s| s.line.as_str()).collect::<Vec<_>>(), ["git commit "], "{next:?}");
        assert!(r.next_after("/elsewhere", NOW, 3).is_empty(), "nothing was run there");
        assert!(r.next_after("/p", NOW + 7200, 3).is_empty(), "too long after the last command");
        // The same line twice makes the whole line the prediction.
        let base = NOW - 5000;
        let again = [(base, "/q", "make"), (base + 10, "/q", "make test"), (base + 100, "/q", "make"), (base + 110, "/q", "make test"), (NOW - 10, "/q", "make")];
        assert_eq!(runs(&again).next_after("/q", NOW, 3)[0].line, "make test");
    }

    #[test]
    fn values_given_to_an_option_come_back_without_secrets() {
        let r = runs(&[
            (NOW - 300, "/a", "kubectl -n staging get pods"),
            (NOW - 200, "/b", "kubectl -n prod get pods"),
            (NOW - 100, "/a", "kubectl --namespace=staging logs web"),
            (NOW - 90, "/a", "kubectl -n \"my ns\" get pods"),
            (NOW - 80, "/a", "kubectl -n -oops get pods"),
            (NOW - 70, "/a", "kubectl -n AKIAABCDEFGHIJKLMNOP1234 get pods"),
            (NOW - 60, "/a", "kubectl --token abc get pods"),
            (NOW - 50, "/a", "helm -n other"),
        ]);
        assert_eq!(r.values_for("kubectl", "-n", "/a", NOW), ["my ns", "staging", "prod"], "this folder first; ties alphabetical");
        assert_eq!(r.values_for("kubectl", "--namespace", "/a", NOW), ["staging"]);
        assert!(r.values_for("kubectl", "--token", "/a", NOW).is_empty(), "options named like secrets are never recalled");
        assert!(r.values_for("kubectl", "-x", "/a", NOW).is_empty());
    }

    #[test]
    fn whole_lines_are_offered_at_the_end_of_the_line_and_as_a_prediction_at_an_empty_prompt() {
        let r = runs(&[(NOW - 90, "/a", "cargo test -p goh-core"), (NOW - 80, "/a", "cargo test -p goh-core"), (NOW - 70, "/a", "cargo build")]);
        let at = |buffer: &str, cursor| candidates(&r, buffer, cursor, "/a", NOW);
        let found = at("cargo t", 7);
        assert_eq!((found.len(), found[0].label.as_str(), found[0].kind, found[0].replace_end_byte), (1, "cargo test -p goh-core", Kind::History, 7));
        assert_eq!(found[0].description.as_deref(), Some("history · 2× · this folder"));
        assert!(at("cargo t", 3).is_empty(), "only at the end of the line");
        assert!(at("", 0).is_empty(), "nothing is predicted from a record this thin");
        assert!(candidates(&Runs::default(), "cargo t", 7, "/a", NOW).is_empty());
    }

    #[test]
    fn lines_that_did_their_job_once_are_not_offered_again() {
        for once in [
            "brew install jq",
            "sudo apt-get install -y ripgrep",
            "npm install -D vitest",
            "HOMEBREW_NO_AUTO_UPDATE=1 brew install --cask iterm2",
            "pip install requests",
            "uv pip install ruff",
            "cargo install ripgrep",
            "git clone https://github.com/a/b.git",
            "git checkout -b fix/login",
            "git branch -d old",
            "git tag v1.2.0",
        ] {
            assert!(done_once(once), "{once:?}");
        }
        for again in [
            "brew update",
            "brew upgrade",
            "npm install",
            "npm i --legacy-peer-deps",
            "pip install -r requirements.txt",
            "pip install -e .",
            "cargo install --path crates/goh-daemon",
            "go install ./...",
            "git checkout main",
            "git branch -a",
            "git tag -l",
            "rm -rf node_modules",
            "brew",
        ] {
            assert!(!done_once(again), "{again:?}");
        }
        let r = runs(&[(NOW - 90, "/a", "brew install jq"), (NOW - 80, "/a", "brew install --cask iterm2"), (NOW - 70, "/a", "brew upgrade")]);
        let found = r.lines_starting("brew ", "/a", NOW, 3);
        assert_eq!(found.iter().map(|s| s.line.as_str()).collect::<Vec<_>>(), ["brew upgrade"]);
    }

    #[test]
    fn a_prediction_is_never_a_line_that_did_its_job_once() {
        let mut lines = Vec::new();
        for i in 0..3 {
            let t = NOW - 100_000 + i * 1000;
            lines.extend([(t, "/p", "git pull"), (t + 5, "/p", "npm install left-pad")]);
        }
        lines.push((NOW - 60, "/p", "git pull"));
        let next = runs(&lines).next_after("/p", NOW, 3);
        // The same install followed each time; only the command is offered, not the package installed already.
        assert_eq!(next.iter().map(|s| s.line.as_str()).collect::<Vec<_>>(), ["npm install "], "{next:?}");
    }

    #[test]
    fn the_zsh_history_is_read_with_the_same_filters_and_no_folder() {
        let mut file = b": 1700000000:0;git status\n: 1700000100:2;brew install jq\n".to_vec();
        file.extend(b": 1700000200:0;echo \xe1\xba\x83\x81\n"); // `ạ` (E1 BA A1): zsh writes A1 as 0x83, A1 ^ 0x20
        file.extend(b": 1700000300:0;export API_KEY=abc\n: 1700000400:0;for f in *; do \\\n  echo $f\\\ndone\n");
        file.extend(b"git log --oneline\n");
        let runs = parse_shell_history(&file, 1_800_000_000);
        let lines: Vec<(u64, &str, &str)> = runs.iter().map(|r| (r.at, r.cwd.as_str(), r.line.as_str())).collect();
        assert_eq!(lines[..2], [(1_700_000_000, "", "git status"), (1_700_000_100, "", "brew install jq")]);
        assert_eq!(lines[2].2, "echo ạ", "unmetafied back to UTF-8");
        assert_eq!(lines[3], (1_799_999_999, "", "git log --oneline"), "a plain line is dated just before the file's time");
        assert_eq!(lines.len(), 4, "secrets and commands over several lines are left out: {lines:?}");
    }

    #[test]
    fn the_shell_history_counts_until_goh_keeps_its_own_record() {
        let shell = parse_shell_history(b": 100:0;cargo build --release\n: 300:0;cargo test -p goh-core\n", 0);
        let own = runs(&[(200, "/a", "cargo test -p goh-core")]).with_shell_history(&shell);
        let found = own.lines_starting("cargo ", "/a", 400, 3);
        assert_eq!(found.iter().map(|s| (s.line.as_str(), s.count, s.here)).collect::<Vec<_>>(), [("cargo test -p goh-core", 1, true), ("cargo build --release", 1, false)], "the shell's line from after the record began is not counted twice");
        assert!(runs(&[]).with_shell_history(&shell).next_after("/a", 400, 3).is_empty(), "no folder, so no prediction from it");
    }

    #[test]
    fn command_keys_ignore_arguments() {
        assert_eq!(command_key("git add ."), "git add");
        assert_eq!(command_key("cargo test -p x"), "cargo test");
        assert_eq!(command_key("cd src"), "cd");
        assert_eq!(command_key("make"), "make");
        assert_eq!(command_key("npm run build"), "npm run");
    }
}
