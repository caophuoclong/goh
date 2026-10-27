//! IPC frames (docs/schemas/ipc.schema.json): 4-byte big-endian length + JSON.

use std::io::{self, Read, Write};

use goh_core::protocol::Candidate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_FRAME: usize = 16 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Complete {
        v: u32,
        session_id: String,
        generation: u64,
        shell: String,
        cwd: String,
        buffer: String,
        cursor_byte: usize,
        editor_state: String,
        /// As-you-type request: answer with candidates only when the line is a recognised command in
        /// progress (`goh_core::Completion::valid`); otherwise none. A plain request ignores validity.
        #[serde(default)]
        live: bool,
    },
    Accept {
        v: u32,
        session_id: String,
        generation: u64,
        suggestion_id: String,
        buffer_hash: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Results { v: u32, session_id: String, generation: u64, suggestions: Vec<Candidate> },
    Edit {
        v: u32,
        session_id: String,
        generation: u64,
        replace_start_byte: usize,
        replace_end_byte: usize,
        insert_text: String,
        cursor_byte: usize,
    },
    Stale { v: u32, session_id: String, generation: u64 },
}

pub fn buffer_hash(buffer: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(buffer.as_bytes()))
}

/// `Ok(None)` on a clean end of stream. Frames over [`MAX_FRAME`] are an error.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn write_frame<W: Write>(w: &mut W, msg: &impl Serialize) -> io::Result<()> {
    let body = serde_json::to_vec(msg).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    w.write_all(&(body.len() as u32).to_be_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

pub fn read_message<T: for<'de> Deserialize<'de>, R: Read>(r: &mut R) -> io::Result<Option<T>> {
    match read_frame(r)? {
        None => Ok(None),
        Some(body) => serde_json::from_slice(&body).map(Some).map_err(io::Error::other),
    }
}
