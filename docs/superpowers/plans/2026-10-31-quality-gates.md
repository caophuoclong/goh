# Quality Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Catch regressions that tests of behaviour miss: daemons that never exit, a slower keystroke, a heavier binary, and a fish adapter nobody has run.

**Architecture:** Four independent pieces. (1) `gohd` exits once its socket file is removed or replaced, so daemons started by tests or left behind by an upgrade do not live forever. On the machine this plan was written on, 29 `target/debug/gohd` processes from test runs were still alive, some for 40 hours. (2) A release-mode example in `goh-core` times spec loading and per-keystroke completion against fixed budgets. (3) A script holds release binary sizes to a checked-in budget. (4) A test runs the fish adapter in a real fish, and CI installs fish so it runs there.

The matcher is not changed. `crates/goh-core/src/matching.rs` already ranks exact prefix > accent/case-folded prefix > the tightest in-order run (gap-penalised) > one- or two-letter typos, with tests. That covers what a library like nucleo would bring at this scale; the benchmark in Task 2 watches its speed.

**Tech Stack:** Rust (std only), POSIX sh, GitHub Actions (macos-15, the runner the test job already uses).

**Spec:** `docs/features/doctor.md` (daemon lifetime), `docs/features/other-shells.md` (fish), `CONTRIBUTING.md` (checks before push).

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits; end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Workflow actions stay pinned by full commit SHA with the version in a comment, as in `.github/workflows/ci.yml`.
- Shell scripts pass `shellcheck`; CI runs it on `scripts/*.sh`.
- No new crate dependencies.

---

## File Structure

| File | Change |
|---|---|
| `crates/goh-daemon/src/paths.rs` | `exit_when_orphaned` |
| `crates/goh-daemon/src/bin/gohd.rs` | Calls it after binding |
| `crates/goh-daemon/tests/ctl_cold_start.rs` | Test: a daemon whose socket is removed exits |
| `crates/goh-core/examples/bench.rs` | Load and keystroke timing with budgets |
| `scripts/check-binary-size.sh`, `scripts/binary-size-budget.txt` | Size budget |
| `crates/goh-daemon/tests/fish_adapter.rs` | fish adapter end to end |
| `.github/workflows/ci.yml` | Install fish; new `budgets` job |
| `CONTRIBUTING.md`, `docs/features/doctor.md`, `docs/features/other-shells.md`, `adapters/fish/README.md` | Docs |

---

### Task 1: A daemon with no socket exits

**Files:**
- Modify: `crates/goh-daemon/src/paths.rs`, `crates/goh-daemon/src/bin/gohd.rs:25-35`
- Test: `crates/goh-daemon/tests/ctl_cold_start.rs`

**Interfaces:**
- Produces: `goh_daemon::paths::exit_when_orphaned(socket: PathBuf)`, which starts a thread that checks every 2 s that `socket` is still the same file (same inode) and calls `std::process::exit(0)` when it is not.

- [ ] **Step 1: Write the failing test**

Append to `crates/goh-daemon/tests/ctl_cold_start.rs`:

```rust
#[test]
fn a_daemon_whose_socket_is_removed_exits() {
    let dir = tempfile::Builder::new().prefix("fo").tempdir_in("/tmp").unwrap();
    let socket = dir.path().join("s");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gohd"))
        .args(["--socket", socket.to_str().unwrap(), "--spec-dir", dir.path().to_str().unwrap()])
        .env("GOH_CONFIG", "/nonexistent")
        .spawn()
        .unwrap();
    let up = Instant::now() + Duration::from_secs(5);
    while !socket.exists() && Instant::now() < up {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(socket.exists(), "the daemon came up");
    fs::remove_file(&socket).unwrap();
    let gone = Instant::now() + Duration::from_secs(6);
    while child.try_wait().unwrap().is_none() && Instant::now() < gone {
        std::thread::sleep(Duration::from_millis(50));
    }
    let status = child.try_wait().unwrap();
    if status.is_none() {
        let _ = child.kill();
    }
    assert!(status.is_some_and(|s| s.success()), "it exits on its own once nobody can reach it");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --test ctl_cold_start a_daemon_whose_socket_is_removed_exits`
Expected: FAIL: the daemon is still running after 6 s.

- [ ] **Step 3: Implement**

Append to `crates/goh-daemon/src/paths.rs`:

```rust
/// End the process once `socket` is gone or is another file: nothing can reach this daemon any more (its folder was
/// removed, as a test's temporary one is, or a newer daemon took the path). Checked every 2 s on its own thread.
pub fn exit_when_orphaned(socket: PathBuf) {
    let Ok(ino) = fs::metadata(&socket).map(|m| m.ino()) else { return };
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if fs::metadata(&socket).map(|m| m.ino()).ok() != Some(ino) {
            crate::log::line("the socket was removed or replaced: exiting");
            std::process::exit(0);
        }
    });
}
```

(`paths.rs` already imports `fs`, `MetadataExt` and `PathBuf`.)

In `crates/goh-daemon/src/bin/gohd.rs`, in the `Ok(Some(listener))` arm, right after `log::init();`:

```rust
            paths::exit_when_orphaned(socket.clone());
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p goh-daemon`
Expected: PASS.

- [ ] **Step 5: Spec and commit**

In `docs/features/doctor.md`, set `updated:` and add under `## Changes`: `- unreleased: a daemon whose socket was removed or taken over exits by itself within 2 s.` Add under `## Behaviour` (or the section listing what doctor checks): `- A daemon nobody can reach any more (its socket removed or replaced) exits by itself.`

```bash
git add crates/goh-daemon/src/paths.rs crates/goh-daemon/src/bin/gohd.rs crates/goh-daemon/tests/ctl_cold_start.rs docs/features/doctor.md
git commit -m "fix(daemon): exit when the socket is removed or replaced"
```

- [ ] **Step 6: Clean up the daemons already leaked on this machine**

Only when no `cargo test` is running here or in a worktree (a running test owns its daemon):

```bash
pgrep -fl 'target/debug/gohd' ; pkill -f 'target/debug/gohd'
```

Expected: the old test daemons are listed, then gone. The Homebrew `gohd` (`/opt/homebrew/bin/gohd`) is not matched.

---

### Task 2: Speed budget for loading specs and completing a keystroke

**Files:**
- Create: `crates/goh-core/examples/bench.rs`

**Interfaces:**
- Produces: `cargo run --release -p goh-core --example bench` prints two lines and exits 1 when over budget:
  - `specs N, load X ms (budget 400 ms)`
  - `complete p50 X ms, p95 X ms (budget 5 ms), max X ms`

- [ ] **Step 1: Write the example**

```rust
//! `cargo run --release -p goh-core --example bench`: how long the bundled specs take to load and one keystroke
//! takes to complete, held to budgets CI checks. Exits 1 over a budget.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use goh_core::{bundled_specs, Engine};

/// Lines people type, across small and very large specs (aws, gcloud, kubectl, helm).
const LINES: &[&str] = &[
    "git c", "git checkout ", "git commit --am", "docker r", "docker run -", "kubectl get ", "kubectl logs -",
    "npm run ", "cargo b", "cargo build --", "aws s3 c", "gcloud compute instances ", "brew in", "ls -", "ssh -",
    "tar -x", "terraform pl", "helm upgrade --", "gh pr cr", "sudo git ch", "git chekcout",
];

/// For a release build on the CI runner: loose enough not to flake, tight enough to catch a real slowdown.
const LOAD_BUDGET: Duration = Duration::from_millis(400);
const P95_BUDGET: Duration = Duration::from_millis(5);

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() -> ExitCode {
    let start = Instant::now();
    let specs = bundled_specs();
    let load = start.elapsed();
    let count = specs.len();
    let engine = Engine::new(specs);
    let cwd = std::env::temp_dir();
    let mut times = Vec::new();
    for _ in 0..20 {
        for line in LINES {
            let t = Instant::now();
            let _ = engine.complete_full(line, line.len(), &cwd);
            times.push(t.elapsed());
        }
    }
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!("specs {count}, load {:.1} ms (budget {} ms)", ms(load), LOAD_BUDGET.as_millis());
    println!("complete p50 {:.2} ms, p95 {:.2} ms (budget {} ms), max {:.2} ms", ms(at(0.5)), ms(at(0.95)), P95_BUDGET.as_millis(), ms(at(1.0)));
    if load > LOAD_BUDGET || at(0.95) > P95_BUDGET {
        eprintln!("over budget");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
```

- [ ] **Step 2: Run it**

Run: `cargo run --release -p goh-core --example bench`
Expected: two lines, exit 0. Note the numbers in the commit message. If it is over budget already, stop and report the numbers: that is a finding, not something to hide by raising the budget.

- [ ] **Step 3: Commit**

```bash
git add crates/goh-core/examples/bench.rs
git commit -m "test(core): time spec loading and completion against budgets"
```

---

### Task 3: Size budget for release binaries

**Files:**
- Create: `scripts/check-binary-size.sh`, `scripts/binary-size-budget.txt`

**Interfaces:**
- Produces: `scripts/check-binary-size.sh` reads `name bytes` lines from `scripts/binary-size-budget.txt`, prints each binary's size in `target/release`, exits 1 if one is over.

- [ ] **Step 1: Write the script**

`scripts/check-binary-size.sh`:

```sh
#!/bin/sh
# Fail when a release binary grew past its budget. Budgets are in scripts/binary-size-budget.txt, one
# "name bytes" per line; build first with `cargo build --release --locked -p goh-daemon -p goh-popup`.
set -eu
cd "$(dirname "$0")/.."
status=0
while read -r name budget; do
  case $name in '' | '#'*) continue ;; esac
  size=$(wc -c < "target/release/$name" | tr -d ' ')
  if [ "$size" -gt "$budget" ]; then
    echo "$name: $size bytes, over its budget of $budget"
    status=1
  else
    echo "$name: $size bytes (budget $budget)"
  fi
done < scripts/binary-size-budget.txt
exit "$status"
```

```bash
chmod +x scripts/check-binary-size.sh
```

- [ ] **Step 2: Measure and set the budgets**

```bash
cargo build --release --locked -p goh-daemon -p goh-popup
wc -c target/release/goh target/release/gohd target/release/goh-popup
```

Write `scripts/binary-size-budget.txt` with each size × 1.15, rounded up to the next 100000:

```text
# Release binary budgets in bytes (macOS arm64, the CI runner): measured size + 15%. Raise one only with a reason
# in the commit message.
goh <bytes>
gohd <bytes>
goh-popup <bytes>
```

(`<bytes>` is the computed number for each line; this file holds numbers only.)

- [ ] **Step 3: Run it**

Run: `scripts/check-binary-size.sh && shellcheck scripts/check-binary-size.sh`
Expected: three lines within budget, exit 0, no shellcheck output.

- [ ] **Step 4: Commit**

```bash
git add scripts/check-binary-size.sh scripts/binary-size-budget.txt
git commit -m "build: hold release binaries to a size budget"
```

---

### Task 4: Run the fish adapter in a real fish

**Files:**
- Create: `crates/goh-daemon/tests/fish_adapter.rs`

**Interfaces:**
- Consumes: `common::Fixture` (`crates/goh-daemon/tests/common/mod.rs`): `Fixture::new(locale, true)` serves the `git` spec in-process; `fx.command(prog, args, env)` runs a program with `GOH_CTL`, `GOH_SOCKET` and `HOME` pointing at the fixture.

- [ ] **Step 1: Install fish locally**

```bash
brew install fish
```

- [ ] **Step 2: Write the test**

`crates/goh-daemon/tests/fish_adapter.rs`:

```rust
//! adapters/fish/goh.fish in a real fish: `complete -C` asks for completions the way the Tab key does.
//! Skipped (with a note) when fish or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};

use common::{utf8_locale, Fixture};

fn fish() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("fish")).find(|f| f.is_file()))
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/fish/goh.fish")
}

#[test]
fn fish_completes_a_spec_command_through_goh() {
    let (Some(fish), Some(locale)) = (fish(), utf8_locale()) else {
        eprintln!("skipping: needs fish and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let script = format!("source {}; complete -C 'git chec'", adapter().display());
    let out = fx.command(fish.to_str().unwrap(), &["--no-config", "-c", &script], &[]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "fish failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(text.lines().any(|l| l.starts_with("checkout")), "goh's candidate reaches fish: {text}");
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p goh-daemon --test fish_adapter`
Expected: PASS. If it fails, the adapter has a real bug (it was never run in fish): reproduce by hand with

```bash
cargo build -p goh-daemon && fish --no-config -c "set -gx GOH_CTL $PWD/target/debug/goh; source adapters/fish/goh.fish; complete -C 'git chec'"
```

fix `adapters/fish/goh.fish` until the test passes, and add `- unreleased: <what was broken> fixed.` under `## Changes` in `docs/features/other-shells.md`.

- [ ] **Step 4: Docs**

In `adapters/fish/README.md`, replace the last line (`Not yet verified against a real fish session ...`) with `Checked in CI by crates/goh-daemon/tests/fish_adapter.rs.`. In `docs/features/other-shells.md` set `updated:` and add under `## Changes`: `- unreleased: the fish adapter is tested in a real fish.`

- [ ] **Step 5: Commit**

```bash
git add crates/goh-daemon/tests/fish_adapter.rs adapters/fish docs/features/other-shells.md
git commit -m "test(fish): run the fish adapter in a real fish"
```

---

### Task 5: CI runs the gates

**Files:**
- Modify: `.github/workflows/ci.yml`, `CONTRIBUTING.md`

- [ ] **Step 1: Edit the workflow**

In `.github/workflows/ci.yml`, in job `test-macos`, add before `- run: cargo test --workspace --locked`:

```yaml
      - run: brew install fish
```

Add a job after `test-macos`:

```yaml
  budgets:
    name: Speed and size budgets (release, macOS)
    runs-on: macos-15
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0
        with:
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable
      - uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2.9.3
      - run: cargo build --release --locked -p goh-daemon -p goh-popup
      - run: scripts/check-binary-size.sh
      - run: cargo run --release --locked -p goh-core --example bench
```

- [ ] **Step 2: Lint the workflow**

Run: `actionlint .github/workflows/ci.yml` (install with `brew install actionlint` if missing)
Expected: no output.

- [ ] **Step 3: Document the checks**

In `CONTRIBUTING.md`, in the `## Before you push` code block, add:

```bash
cargo run --release -p goh-core --example bench   # spec loading and per-keystroke time, against budgets
scripts/check-binary-size.sh                      # after a release build: binaries within their size budget
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml CONTRIBUTING.md
git commit -m "ci: run fish tests and the speed and size budgets"
```
