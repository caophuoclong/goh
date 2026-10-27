//! One engine, per-session state, one thread per connection. Never logs buffers.

use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use goh_core::protocol::Candidate;
use goh_core::Engine;

use crate::wire::{buffer_hash, read_message, write_frame, Reply, Request};

const MAX_SESSIONS: usize = 256;
const MAX_CONNECTIONS: usize = 32;
const IDLE_TIMEOUT: Duration = Duration::from_secs(2);

struct Session {
    generation: u64,
    buffer_hash: String,
    candidates: Vec<Candidate>,
}

pub struct Server {
    engine: Engine,
    sessions: Mutex<HashMap<String, Session>>,
    connections: AtomicUsize,
}

fn valid_session(id: &str) -> bool {
    (8..=64).contains(&id.len())
}

impl Server {
    pub fn new(engine: Engine) -> Self {
        Self { engine, sessions: Mutex::new(HashMap::new()), connections: AtomicUsize::new(0) }
    }

    /// `None` means the request is malformed: the caller drops the connection without replying.
    pub fn handle(&self, req: Request) -> Option<Reply> {
        match req {
            Request::Complete { v, session_id, generation, cwd, buffer, cursor_byte, editor_state, live, .. } => {
                if v != 1 || !valid_session(&session_id) {
                    return None;
                }
                let candidates = if editor_state == "editing" {
                    match self.engine.complete_full(&buffer, cursor_byte, Path::new(&cwd)) {
                        // As-you-type requests only get suggestions for a recognised command in progress.
                        Ok(c) if live && c.valid => c.prefix_matches_only().candidates,
                        Ok(c) if !live => c.candidates,
                        _ => vec![],
                    }
                } else {
                    vec![]
                };
                let mut sessions = self.sessions.lock().ok()?;
                if sessions.get(&session_id).is_some_and(|s| generation < s.generation) {
                    return Some(Reply::Stale { v: 1, session_id, generation });
                }
                if sessions.len() >= MAX_SESSIONS && !sessions.contains_key(&session_id) {
                    sessions.clear();
                }
                sessions.insert(
                    session_id.clone(),
                    Session { generation, buffer_hash: buffer_hash(&buffer), candidates: candidates.clone() },
                );
                Some(Reply::Results { v: 1, session_id, generation, suggestions: candidates })
            }
            Request::Accept { v, session_id, generation, suggestion_id, buffer_hash } => {
                if v != 1 || !valid_session(&session_id) {
                    return None;
                }
                let stale = Reply::Stale { v: 1, session_id: session_id.clone(), generation };
                let sessions = self.sessions.lock().ok()?;
                let Some(s) = sessions.get(&session_id) else { return Some(stale) };
                if s.generation != generation || s.buffer_hash != buffer_hash {
                    return Some(stale);
                }
                let Some(c) = s.candidates.iter().find(|c| c.id == suggestion_id) else { return Some(stale) };
                Some(Reply::Edit {
                    v: 1,
                    session_id,
                    generation,
                    replace_start_byte: c.replace_start_byte,
                    replace_end_byte: c.replace_end_byte,
                    cursor_byte: c.replace_start_byte + c.insert_text.len(),
                    insert_text: c.insert_text.clone(),
                })
            }
        }
    }

    fn connection(&self, mut stream: UnixStream) {
        let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IDLE_TIMEOUT));
        while let Ok(Some(req)) = read_message::<Request, _>(&mut stream) {
            let Some(reply) = self.handle(req) else { return };
            if write_frame(&mut stream, &reply).is_err() {
                return;
            }
        }
    }

    pub fn serve(self: Arc<Self>, listener: UnixListener) {
        for stream in listener.incoming().flatten() {
            if self.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                self.connections.fetch_sub(1, Ordering::SeqCst);
                continue; // drop: the client fails open
            }
            let me = Arc::clone(&self);
            std::thread::spawn(move || {
                me.connection(stream);
                me.connections.fetch_sub(1, Ordering::SeqCst);
            });
        }
    }
}
