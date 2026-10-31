# Late Provider Answers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a slow program (Docker on a loaded host, a cluster, tmux, ...) answers after the 120 ms a keystroke waits, its values appear in the open menu by themselves, without another key, and the menu says it is still waiting.

**Architecture:** Today `Providers::ask` (`crates/goh-daemon/src/providers.rs:118`) already finishes a slow query in the background; its answer is only used at the next request. This plan threads one bit, "a program asked for this menu has not answered yet", from `Providers` through the `results` reply (`pending`) and `goh menu --late` (a last line `pending`) to the zsh adapter. The adapter then re-asks once after 300 ms through a `zle -F` timer, while the line is unchanged, at most 16 times (about 5 s, the background deadline). No push channel from the daemon, no new process kept alive.

**Tech Stack:** Rust (`goh-daemon`), zsh adapter, PTY tests.

**Spec:** `docs/features/project-context.md`, `docs/features/zsh-adapter.md`, `docs/schemas/ipc.schema.json`.

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits; end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Typing is never blocked: the re-ask runs from ZLE's idle loop (`zle -F`), and a busy or changed line drops it.
- Only adapters that pass `--late` see the `pending` line; bash, fish, elvish, PowerShell and Nushell output is unchanged.
- The daemon/client socket protocol may change freely between same-version binaries (CONTRIBUTING.md); `goh menu` flags are public, so `--late` is additive.

---

## File Structure

| File | Change |
|---|---|
| `crates/goh-daemon/src/providers.rs` | `values_or_pending`; `ask` reports a missing first answer |
| `crates/goh-daemon/src/wire.rs` | `Reply::Results.pending` |
| `crates/goh-daemon/src/server.rs` | Collects `pending` while completing |
| `docs/schemas/ipc.schema.json` | `pending` in `results` |
| `crates/goh-daemon/src/client.rs`, `crates/goh-daemon/src/doctor.rs` | `menu` returns `(items, pending)` |
| `crates/goh-daemon/src/bin/goh.rs` | `--late` flag prints `pending` |
| `adapters/zsh/goh.zsh` | Re-ask timer, footer hint |
| `crates/goh-daemon/tests/zsh_pty.rs` | End-to-end test with a slow fake Docker |

---

### Task 1: The daemon says when an answer is still coming

**Files:**
- Modify: `crates/goh-daemon/src/providers.rs:65-142`
- Modify: `crates/goh-daemon/src/wire.rs:62-66`, `crates/goh-daemon/src/server.rs:106-142`, `docs/schemas/ipc.schema.json:62-70`

**Interfaces:**
- Produces: `Providers::values_or_pending(&self, provider: Provider, cwd: &Path) -> (Vec<Suggestion>, bool)`, where `true` means the program has been asked and has never answered for this key. `Providers::values` keeps its signature and returns `.0`. `Reply::Results { v, session_id, generation, suggestions, pending: bool }`; `pending` is omitted from JSON when false.

- [ ] **Step 1: Write the failing test**

In `crates/goh-daemon/src/providers.rs`, inside `mod tests`:

```rust
    #[test]
    fn a_first_slow_answer_is_reported_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let docker = script(dir.path(), "sleep 0.5\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'");
        let providers = Providers { docker, home: dir.path().to_path_buf(), ..Providers::default() };
        let (values, pending) = providers.values_or_pending(Provider::DockerContainers, Path::new("/"));
        assert!(values.is_empty() && pending, "nothing yet, and an answer is on its way");
        let deadline = Instant::now() + Duration::from_secs(5);
        while providers.values(Provider::DockerContainers, Path::new("/")).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let (values, pending) = providers.values_or_pending(Provider::DockerContainers, Path::new("/"));
        assert_eq!((values[0].name.as_str(), pending), ("web-1", false));
        assert!(!providers.values_or_pending(Provider::SshHosts, Path::new("/")).1, "files are read at once");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --lib providers::tests::a_first_slow_answer_is_reported_as_pending`
Expected: compile error, no method `values_or_pending`.

- [ ] **Step 3: Implement in `Providers`**

Rename `pub fn values(&self, provider: Provider, cwd: &Path) -> Vec<Suggestion>` to `pub fn values_or_pending(&self, provider: Provider, cwd: &Path) -> (Vec<Suggestion>, bool)` and change its body's returns:

- the kubectl branch: `return self.ask(provider, context, &self.kubectl, Path::new("/"), CLUSTER_TTL, Duration::ZERO);` stays (it now returns a tuple).
- the program branch:

```rust
            let (values, pending) = self.ask(provider, key, bin, dir, TTL, DEADLINE);
            if provider == Provider::FrecentDirs && values.is_empty() {
                // zoxide missing, failing, slow or empty: use goh's own history.
                let own = crate::visits::ranked(&self.visits, crate::visits::now())
                    .into_iter()
                    .map(|name| Suggestion { name, description: None, insert_value: None })
                    .collect();
                return (own, false);
            }
            return (values, pending);
```

- the cache branch: `return vec![];` becomes `return (vec![], false);`, `return values.clone();` becomes `return (values.clone(), false);`, and the final `values` becomes `(values, false)`.

Then add the old name back as a thin wrapper, above `values_or_pending`:

```rust
    pub fn values(&self, provider: Provider, cwd: &Path) -> Vec<Suggestion> {
        self.values_or_pending(provider, cwd).0
    }
```

Change `ask`'s doc comment last sentence and signature:

```rust
    /// ... what it said last otherwise (nothing before its first answer). `true` with the values while it has been
    /// asked and has never answered: an answer is on its way.
    fn ask(&self, provider: Provider, key: String, bin: &Path, dir: &Path, ttl: Duration, wait: Duration) -> (Vec<Suggestion>, bool) {
        let (lock, arrived) = &*self.asked;
        let Ok(mut asked) = lock.lock() else { return (vec![], false) };
```

replace `let Ok((waited, _)) = ... else { return vec![] };` with `... else { return (vec![], false) };`, and the last line with:

```rust
        asked.get(&key).map(|a| (a.values.clone(), a.asking && a.at.is_none())).unwrap_or_default()
```

- [ ] **Step 4: Carry it in the reply**

In `crates/goh-daemon/src/wire.rs`, replace the `Results` variant:

```rust
    Results {
        v: u32,
        session_id: String,
        generation: u64,
        suggestions: Vec<Candidate>,
        /// A program asked for these suggestions (Docker, a cluster, ...) has not answered yet; asking again soon
        /// may bring more.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pending: bool,
    },
```

In `crates/goh-daemon/src/server.rs`, in the `Request::Complete` arm, before `let candidates = if editor_state == "editing" {`:

```rust
                let pending = std::cell::Cell::new(false);
                let values = |p, cwd: &Path| {
                    let (values, waiting) = self.providers.values_or_pending(p, cwd);
                    pending.set(pending.get() || waiting);
                    values
                };
```

replace `&|p, cwd| self.providers.values(p, cwd)` in the `complete_in` call with `&values`, and the reply with:

```rust
                Some(Reply::Results { v: 1, session_id, generation, suggestions: candidates, pending: pending.get() })
```

In `docs/schemas/ipc.schema.json`, inside `results.properties`, after `"suggestions"`:

```json
        "pending": { "type": "boolean", "description": "A program asked for values has not answered yet; asking again soon may bring more." }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p goh-daemon`
Expected: PASS (other code destructures `Reply::Results { .. }` and is unaffected).

- [ ] **Step 6: Commit**

```bash
git add crates/goh-daemon/src/providers.rs crates/goh-daemon/src/wire.rs crates/goh-daemon/src/server.rs docs/schemas/ipc.schema.json
git commit -m "feat(daemon): tell the client when a program's answer is still coming"
```

---

### Task 2: `goh menu --late`

**Files:**
- Modify: `crates/goh-daemon/src/client.rs:124-131`, `crates/goh-daemon/src/doctor.rs:204-213`, `crates/goh-daemon/src/bin/goh.rs:6,644-680`

**Interfaces:**
- Consumes: `Reply::Results.pending` (Task 1).
- Produces: `client::menu(...) -> io::Result<(Vec<Item>, bool)>`. `goh menu ... --late` prints a last line `pending` when the daemon is still waiting, and then exits 0 even with no item lines.

- [ ] **Step 1: Write the failing test**

The test needs a real `gohd` (bundled `docker` spec, `PATH` with a slow fake `docker`), so it goes in `crates/goh-daemon/tests/ctl_cold_start.rs`, where `goh` cold-starts its own daemon. The in-process daemon of `Fixture::new(_, true)` only knows `git`.

```rust
#[test]
fn menu_late_prints_pending_while_docker_has_not_answered() {
    let dir = tempfile::Builder::new().prefix("fl").tempdir_in("/tmp").unwrap();
    let (socket, specs, bin) = (dir.path().join("s"), dir.path().join("specs"), dir.path().join("bin"));
    fs::create_dir_all(&specs).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nsleep 1\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'\n").unwrap();
    fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let goh_dir = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap();
    let path = format!("{}:{}:/usr/bin:/bin", bin.display(), goh_dir.display());
    let run = |late: bool| {
        let mut args = vec!["menu", "--session", "test-late-1", "--cwd", "/", "--cursor-byte", "12", "--buffer", "docker exec "];
        if late {
            args.push("--late");
        }
        let out = Command::new(env!("CARGO_BIN_EXE_goh"))
            .args(&args)
            .env("GOH_SOCKET", &socket)
            .env("GOH_SPEC_DIR", &specs)
            .env("GOH_CONFIG", "/nonexistent")
            .env("PATH", &path)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let first = run(true);
    assert_eq!(first.lines().last(), Some("pending"), "{first}");
    assert!(!run(false).contains("pending"), "only --late prints it");
    std::thread::sleep(Duration::from_millis(1500));
    let later = run(true);
    assert!(later.contains("web-1") && !later.contains("pending"), "{later}");
    kill_socket_holder(&socket);
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --test ctl_cold_start menu_late_prints_pending_while_docker_has_not_answered`
Expected: FAIL: `goh` exits non-zero on the unknown flag `--late`, so stdout is empty.

- [ ] **Step 3: Implement**

`crates/goh-daemon/src/client.rs`:

```rust
pub fn menu(sock: &Path, session: &str, cwd: &str, buffer: &str, cursor_byte: usize, limit: usize, live: bool) -> io::Result<(Vec<Item>, bool)> {
    let (_, _, reply) = complete(sock, session, cwd, buffer, cursor_byte, live)?;
    let Reply::Results { suggestions, pending, .. } = reply else { return Ok((vec![], false)) };
    Ok((keep_history(items(suggestions, buffer), limit), pending))
}
```

and update its doc comment's first line to `/// Candidates for the menu, best first, with invalid insertions dropped, and whether a program has yet to answer.`

`crates/goh-daemon/src/doctor.rs`, in `round_trip`:

```rust
        Ok((items, _)) if items.iter().any(|i| i.label == TEST_EXPECT) => RoundTrip::Offered { millis },
```

`crates/goh-daemon/src/bin/goh.rs`:
- line 6 usage: `//!   goh menu     --session S --cwd D --cursor-byte N --buffer B [--limit N] [--live] [--late]`
- `let (mut live, mut labels) = (false, false);` becomes `let (mut live, mut labels, mut late) = (false, false, false);`
- after `"--labels" => labels = true,` add:

```rust
            // `--late`: print a last line `pending` while a program asked for values has not answered, so the
            // adapter can ask again; other adapters never see it.
            "--late" => late = true,
```

- replace the `"menu" => ...` arm with:

```rust
        "menu" => client::menu(&sock, &session, &cwd, &buffer, cursor, limit, live).map(|(mut items, pending)| {
            items.retain(|i| labels || i.kind != "history" || i.label == i.edit.insert);
            let pending = pending && late;
            for i in &items {
                let e = &i.edit;
                let ranges: Vec<String> = i.matches.iter().map(|r| format!("{}-{}", r[0], r[1])).collect();
                let flag = |b: bool| if b { "1" } else { "" };
                println!(
                    "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
                    e.start, e.end, e.cursor, i.kind, i.label, e.insert, i.description, ranges.join(","), flag(i.takes_value), flag(i.used)
                );
            }
            if pending {
                println!("pending");
            }
            (!items.is_empty() || pending).then_some(())
        }),
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p goh-daemon`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/goh-daemon/src/client.rs crates/goh-daemon/src/doctor.rs crates/goh-daemon/src/bin/goh.rs crates/goh-daemon/tests/ipc.rs
git commit -m "feat(cli): goh menu --late marks a menu still waiting for a program"
```

---

### Task 3: zsh shows late answers without another key

**Files:**
- Modify: `adapters/zsh/goh.zsh` (`_goh_render` footer near `:491-501`, `_goh_live_refresh` at `:787-819`, `_goh_hook_finish` at `:846`, `goh-load` at `:959`)
- Test: `crates/goh-daemon/tests/zsh_pty.rs`

**Interfaces:**
- Consumes: `goh menu --late` (Task 2).
- Produces: widgets `_goh_on_late`; helpers `_goh_late_wait`, `_goh_late_stop`; footer shows `…` after the position while waiting.

- [ ] **Step 1: Write the failing test**

Append to `crates/goh-daemon/tests/zsh_pty.rs`:

`Fixture::new(_, false)` starts no in-process daemon: the shell's first request cold-starts a real `gohd` with the bundled `docker` spec and this `PATH`.

```rust
#[test]
fn a_slow_docker_fills_the_open_menu_without_another_key() {
    let fx = Fixture::new(require_env!(), false);
    let bin = fx.path("slowbin");
    fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nsleep 0.6\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'\n").unwrap();
    fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let goh_dir = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap().display().to_string();
    let path = format!("{}:{goh_dir}:/usr/bin:/bin", bin.display());
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("PATH", &path)], true);
    menu_dump(&fx, &mut sh);
    sh.send("docker exec ", 1500); // the first key also starts the daemon
    // No key after this: the answer must arrive through the adapter's own re-ask.
    sh.drain(Duration::from_millis(1500), Duration::from_secs(3));
    let (rows, _) = menu(&fx, &mut sh);
    kill_socket_holder(&fx.socket);
    assert!(rows.iter().any(|r| r.contains("web-1")), "the late container shows by itself: {rows:?}");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --test zsh_pty a_slow_docker_fills_the_open_menu_without_another_key`
Expected: FAIL: `web-1` is not in the rows (today it only shows after the next key).

- [ ] **Step 3: Implement the re-ask**

In `adapters/zsh/goh.zsh`, add after `_goh_pop_hide` (around line 268):

```zsh
# ---- late answers ----------------------------------------------------------------------------
# A program the menu asked (Docker, a cluster, tmux, ...) had not answered: ask again in 300 ms while the line stays
# the same, so its values show without another key. One timer at a time, at most 16 tries (about 5 s) per line.
typeset -gi _goh_late_fd=-1 _goh_late_tries=0 _goh_late=0
typeset -g _goh_late_line

_goh_late_stop() {
  (( _goh_late_fd >= 0 )) || return 0
  zle -F $_goh_late_fd 2>/dev/null
  exec {_goh_late_fd}<&-
  _goh_late_fd=-1
}

_goh_late_wait() {
  if [[ $_goh_late_line != "$CURSOR:$BUFFER" ]]; then _goh_late_line="$CURSOR:$BUFFER" _goh_late_tries=0; fi
  (( _goh_late_fd >= 0 )) && return 0
  (( ++_goh_late_tries > 16 )) && return 0
  exec {_goh_late_fd}< <(command sleep 0.3; print)
  zle -F -w $_goh_late_fd _goh_on_late
}

_goh_on_late() {
  emulate -L zsh
  _goh_late_stop
  [[ $_goh_late_line == "$CURSOR:$BUFFER" && $CONTEXT == start ]] || return 0
  _goh_live_refresh
  zle -R
}
```

In `_goh_live_refresh`, change the query to pass `--late` and read the marker:

```zsh
    out=$(command $_GOH_CTL menu --labels --live --late --session $_GOH_SESSION --cwd $PWD --limit 50 \
          --cursor-byte $cb --buffer "$BUFFER" 2>/dev/null) || out=
    _goh_late=0
    [[ ${out##*$'\n'} == pending ]] && _goh_late=1
```

and at the end of the `{ ... }` block, just before `} always {`, add:

```zsh
    (( _goh_late )) && _goh_late_wait
```

In `_goh_render`, after `(( $#_goh_label >= 50 )) && pos+='+'   # menus ask for 50: there may be more`, add:

```zsh
  (( _goh_late )) && pos+=' …'   # a program has not answered yet: more may come
```

In `_goh_hook_finish`, stop the timer first:

```zsh
_goh_hook_finish() { _goh_late_stop; _goh_close no-redisplay; _goh_drop_pred; _goh_seen_cur=-1; return 0 }
```

In `goh-load`, after `zle -N _goh_enter; zle -N _goh_left`, add:

```zsh
  zle -N _goh_on_late
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p goh-daemon --test zsh_pty`
Expected: PASS, the new test and the existing ones.

- [ ] **Step 5: Commit**

```bash
git add adapters/zsh/goh.zsh crates/goh-daemon/tests/zsh_pty.rs
git commit -m "feat(zsh): show a slow program's values when they arrive"
```

---

### Task 4: Feature specs

**Files:**
- Modify: `docs/features/project-context.md`, `docs/features/zsh-adapter.md`

- [ ] **Step 1: Update the specs**

`docs/features/project-context.md`: replace the bullet that ends `its answer shows from the next keystroke, so a slow Docker host still lists its containers.` with:

```markdown
- Programs asked for values (docker, git, zoxide, tmux, systemctl, ps) get 120 ms per keystroke; a slower one finishes
  in the background (up to 5 s). In zsh its values join the open menu when they arrive, with no key pressed (the
  footer shows `…` while one is still answering); other shells see them from the next key.
```

Set `updated:` and add `- unreleased: in zsh, values from a slow program join the open menu by themselves.` under `## Changes`.

`docs/features/zsh-adapter.md`: under `## Behaviour` add `- A program still answering (a slow Docker, a cluster) shows `…` in the footer; its values appear when they arrive.`; set `updated:`; add `- unreleased: late values from slow programs appear without another key.` under `## Changes`.

- [ ] **Step 2: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add docs/features/project-context.md docs/features/zsh-adapter.md
git commit -m "docs: describe late values in the menu"
```
