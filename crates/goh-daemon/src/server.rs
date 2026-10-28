//! One engine, per-session state, one thread per connection. Never logs buffers.

use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use goh_core::protocol::Candidate;
use goh_core::shellctx::ShellContext;
use goh_core::Engine;

use crate::wire::{buffer_hash, read_message, write_frame, Reply, Request};

const MAX_SESSIONS: usize = 256;
const MAX_CONNECTIONS: usize = 32;
const IDLE_TIMEOUT: Duration = Duration::from_secs(2);
const USAGE_TTL: Duration = Duration::from_secs(2);

struct Session {
    generation: u64,
    buffer_hash: String,
    candidates: Vec<Candidate>,
}

pub struct Server {
    engine: Arc<Engine>,
    providers: crate::providers::Providers,
    runs: Arc<crate::runs::RunsCache>,
    snippets: crate::snippets::SnippetCache,
    autospecs: crate::autospec::AutoSpecs,
    sessions: Mutex<HashMap<String, Session>>,
    shells: crate::shell::Shells,
    contexts: Mutex<HashMap<String, Arc<ShellContext>>>,
    usage: Mutex<(Instant, Arc<crate::usage::Usage>)>,
    connections: AtomicUsize,
}

fn valid_session(id: &str) -> bool {
    (8..=64).contains(&id.len())
}

impl Server {
    /// What you picked before, re-read at most every couple of seconds. Empty when learning is switched off.
    fn usage(&self) -> Arc<crate::usage::Usage> {
        let Ok(mut cached) = self.usage.lock() else { return Arc::default() };
        if cached.0.elapsed() >= USAGE_TTL {
            let on = crate::usage::enabled();
            let loaded = if on { crate::usage::Usage::load(&crate::usage::path(), crate::visits::now()) } else { Default::default() };
            *cached = (Instant::now(), Arc::new(loaded));
        }
        Arc::clone(&cached.1)
    }

    /// Put what you pick most for this command first, without letting it reorder across match quality.
    fn rank_by_usage(&self, cwd: &str, buffer: &str, candidates: &mut [Candidate]) {
        let usage = self.usage();
        for c in candidates.iter_mut() {
            c.score += usage.boost(cwd, buffer.get(..c.replace_start_byte).unwrap_or(""), &c.label);
        }
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score)); // stable: ties keep the engine's order
    }

    /// Warnings (and the context a command acts on) in the descriptions of the risky suggestions.
    fn warn(&self, cwd: &str, buffer: &str, cursor: usize, shell: &ShellContext, found: &mut [Candidate]) {
        use crate::risk::Target;
        let (line, delta) = shell.expand(buffer, cursor).map_or((buffer.to_string(), 0), |e| (e.buffer, e.delta));
        crate::risk::annotate(found, &line, delta, &|target| match target {
            Target::Kube => self
                .providers
                .values(goh_core::spec::Provider::KubeContexts, Path::new(cwd))
                .into_iter()
                .find(|s| s.description.as_deref() == Some("current context"))
                .map(|s| format!("context: {}", s.name)),
            Target::Terraform => crate::risk::terraform_workspace(Path::new(cwd)).map(|w| format!("workspace: {w}")),
            Target::None => None,
        });
    }

    pub fn new(engine: Engine) -> Self {
        Self::with_runs(engine, Arc::new(crate::runs::RunsCache::new(crate::runs::path())))
    }

    /// Like [`Server::new`], with the record of commands given (so a test can supply its own).
    pub fn with_runs(engine: Engine, runs: Arc<crate::runs::RunsCache>) -> Self {
        // Values you gave an option before come from the (opt-in) record of commands.
        let recall = Arc::clone(&runs);
        let engine = Arc::new(engine.with_recall(Box::new(move |command, option, cwd| {
            recall.get().values_for(command, option, &cwd.to_string_lossy(), crate::visits::now())
        })));
        Self { engine, runs, snippets: Default::default(), autospecs: Default::default(), providers: Default::default(), sessions: Mutex::new(HashMap::new()), shells: Default::default(), contexts: Mutex::new(HashMap::new()), usage: Mutex::new((Instant::now() - USAGE_TTL, Arc::default())), connections: AtomicUsize::new(0) }
    }

    /// `None` means the request is malformed: the caller drops the connection without replying.
    pub fn handle(&self, req: Request) -> Option<Reply> {
        match req {
            Request::Complete { v, session_id, generation, cwd, buffer, cursor_byte, editor_state, live, .. } => {
                if v != 1 || !valid_session(&session_id) {
                    return None;
                }
                let candidates = if editor_state == "editing" {
                    let shell = self.contexts.lock().ok().and_then(|c| c.get(&session_id).cloned()).unwrap_or_default();
                    let mut found = match self.engine.complete_in(&buffer, cursor_byte, Path::new(&cwd), &|p, cwd| self.providers.values(p, cwd), &shell) {
                        // As-you-type requests only get suggestions for a recognised command in progress.
                        Ok(c) if live && c.valid => c.prefix_matches_only().candidates,
                        Ok(c) if !live => c.candidates,
                        _ => vec![],
                    };
                    self.rank_by_usage(&cwd, &buffer, &mut found);
                    self.warn(&cwd, &buffer, cursor_byte, &shell, &mut found);
                    self.autospecs.consider(&self.engine, &buffer, cursor_byte, &shell);
                    // Your snippets lead; whole lines from the record of commands follow the rest.
                    let mut all = if buffer.is_char_boundary(cursor_byte) { crate::snippets::candidates(&self.snippets.get(), &buffer, cursor_byte, live) } else { vec![] };
                    all.extend(found);
                    all.extend(crate::runs::candidates(&self.runs.get(), &buffer, cursor_byte, &cwd, crate::visits::now()));
                    all
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
            Request::Hello { v } => (v == 1).then(|| Reply::Hello { v: 1, version: env!("CARGO_PKG_VERSION").to_string(), pid: std::process::id() }),
            Request::Context { v, session_id, aliases, functions, builtins, path, env } => {
                if v != 1 || !valid_session(&session_id) {
                    return None;
                }
                let ctx = self.shells.context(&aliases, &functions, &builtins, &path, &env);
                let mut contexts = self.contexts.lock().ok()?;
                if contexts.len() >= MAX_SESSIONS && !contexts.contains_key(&session_id) {
                    contexts.clear();
                }
                contexts.insert(session_id, Arc::new(ctx));
                Some(Reply::Ack { v: 1 })
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
                    cursor_byte: c.replace_start_byte + c.cursor_offset.unwrap_or(c.insert_text.len()),
                    insert_text: c.insert_text.clone(),
                })
            }
        }
    }

    fn connection(&self, mut stream: UnixStream) {
        let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IDLE_TIMEOUT));
        while let Ok(Some(req)) = read_message::<Request, _>(&mut stream) {
            let Some(reply) = self.handle(req) else {
                crate::log::throttled("malformed", "dropped a request the daemon could not accept (bad version or session id)");
                return;
            };
            if write_frame(&mut stream, &reply).is_err() {
                return;
            }
        }
    }

    pub fn serve(self: Arc<Self>, listener: UnixListener) {
        for stream in listener.incoming().flatten() {
            if self.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                self.connections.fetch_sub(1, Ordering::SeqCst);
                crate::log::throttled("connections", "too many open connections: dropping new ones (clients fail open)");
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
