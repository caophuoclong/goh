//! Adapter-side client. Every failure is an `Err`/`None`: the caller leaves the shell alone.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::wire::{read_message, write_frame, Reply, Request};

const TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub cursor: usize,
    pub insert: String,
}

fn connect(sock: &Path) -> io::Result<UnixStream> {
    let s = UnixStream::connect(sock)?;
    s.set_read_timeout(Some(TIMEOUT))?;
    s.set_write_timeout(Some(TIMEOUT))?;
    Ok(s)
}

fn round_trip(s: &mut UnixStream, req: &Request) -> io::Result<Reply> {
    write_frame(s, req)?;
    read_message(s)?.ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "daemon closed connection"))
}

/// Monotonic across processes (an edit is always newer than the previous one).
fn next_generation() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

/// Ask for candidates; returns the stream (for a follow-up accept), the generation and the reply.
pub fn complete(sock: &Path, session: &str, cwd: &str, buffer: &str, cursor_byte: usize, live: bool) -> io::Result<(UnixStream, u64, Reply)> {
    let mut s = connect(sock)?;
    let generation = next_generation();
    let reply = round_trip(
        &mut s,
        &Request::Complete {
            v: 1,
            session_id: session.into(),
            generation,
            shell: "zsh".into(),
            cwd: cwd.into(),
            buffer: buffer.into(),
            cursor_byte,
            editor_state: "editing".into(),
            live,
        },
    )?;
    Ok((s, generation, reply))
}

/// The daemon's version and pid. A daemon from before `Hello` closes the connection, which is an `Err` here.
pub fn hello(sock: &Path) -> io::Result<(String, u32)> {
    let mut s = connect(sock)?;
    match round_trip(&mut s, &Request::Hello { v: 1 })? {
        Reply::Hello { version, pid, .. } => Ok((version, pid)),
        _ => Err(io::Error::other("unexpected reply")),
    }
}

/// Tell the daemon what this shell knows. Frames are small, so the lists are cut to fit.
pub fn context(sock: &Path, session: &str, aliases: Vec<[String; 2]>, functions: Vec<String>, builtins: Vec<String>, path: String, env: Vec<String>) -> io::Result<()> {
    let mut req = Request::Context { v: 1, session_id: session.into(), aliases, functions, builtins, path, env };
    // The frame limit is 16 KiB: drop from the end until it fits (variables first, then functions, then aliases).
    while serde_json::to_vec(&req).map_or(0, |b| b.len()) > crate::wire::MAX_FRAME {
        let Request::Context { aliases, functions, env, .. } = &mut req else { unreachable!() };
        if !env.is_empty() {
            env.truncate(env.len() / 2);
        } else if !functions.is_empty() {
            functions.truncate(functions.len() / 2);
        } else if !aliases.is_empty() {
            aliases.truncate(aliases.len() / 2);
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "context too large"));
        }
    }
    let mut s = connect(sock)?;
    match round_trip(&mut s, &req)? {
        Reply::Ack { .. } => Ok(()),
        _ => Err(io::Error::other("unexpected reply")),
    }
}

/// One validated, ready-to-apply candidate. Text fields are single-line.
#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub edit: Edit,
    pub label: String,
    pub description: String,
    pub kind: String,
    /// UTF-8 byte ranges of `label` that matched what was typed.
    pub matches: Vec<[usize; 2]>,
    /// An option followed by a value, as opposed to a flag.
    pub takes_value: bool,
    /// Picked before for this command.
    pub used: bool,
}

fn single_line(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// Never trust the wire: the span must be valid in *our* buffer and the text must be one line.
fn valid_edit(buffer: &str, e: &Edit) -> bool {
    e.start <= e.end
        && e.end <= buffer.len()
        && buffer.is_char_boundary(e.start)
        && buffer.is_char_boundary(e.end)
        && (e.start..=e.start + e.insert.len()).contains(&e.cursor)
        && e.insert.is_char_boundary(e.cursor - e.start)
        && !e.insert.is_empty()
        && !e.insert.chars().any(char::is_control)
}

/// Candidates for the menu, best first, with invalid insertions dropped. The adapter applies them
/// locally while the buffer is unchanged, so no `accept` round trip is needed.
pub fn menu(sock: &Path, session: &str, cwd: &str, buffer: &str, cursor_byte: usize, limit: usize, live: bool) -> io::Result<Vec<Item>> {
    let (_, _, reply) = complete(sock, session, cwd, buffer, cursor_byte, live)?;
    let Reply::Results { suggestions, .. } = reply else { return Ok(vec![]) };
    Ok(suggestions
        .into_iter()
        .filter_map(|c| {
            let edit = Edit {
                start: c.replace_start_byte,
                end: c.replace_end_byte,
                cursor: c.replace_start_byte + c.cursor_offset.unwrap_or(c.insert_text.len()),
                insert: c.insert_text,
            };
            valid_edit(buffer, &edit).then(|| Item {
                edit,
                label: single_line(&c.label, 256),
                description: single_line(c.description.as_deref().unwrap_or(""), 120),
                kind: format!("{:?}", c.kind).to_lowercase(),
                matches: c.match_ranges,
                takes_value: c.takes_value,
                used: c.used,
            })
        })
        .take(limit)
        .collect())
}
