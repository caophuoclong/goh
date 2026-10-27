//! Wire types for `results` frames (docs/schemas/ipc.schema.json). Offsets are UTF-8 bytes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Command,
    Subcommand,
    Option,
    Argument,
    File,
    Directory,
    /// A whole command line seen before (or predicted to come next); replaces the line.
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    #[serde(rename = "bundled")]
    Bundled,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "filesystem")]
    Filesystem,
    #[serde(rename = "built-in generator")]
    Generator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: Kind,
    pub insert_text: String,
    pub replace_start_byte: usize,
    pub replace_end_byte: usize,
    pub match_ranges: Vec<[usize; 2]>,
    pub score: f64,
    pub source: Source,
    /// Where the cursor goes within `insert_text` (a snippet's `{{cursor}}`); the end of it when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub session_id: String,
    pub generation: u64,
    pub suggestions: Vec<Candidate>,
}

impl Response {
    pub fn new(session_id: String, generation: u64, suggestions: Vec<Candidate>) -> Self {
        Self { v: 1, kind: "results", session_id, generation, suggestions }
    }
}
