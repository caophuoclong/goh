# 16 — Implementation Guide: from docs to first working build

**Status:** Proposed, pending R0 spikes. Everything here is a plan, not an existing codebase. Commands and snippets are illustrative and have not been run against a real repository.

## Principles

1. **Prove the risky parts first.** Shell input safety, IPC, and fail-open behaviour come before any UI ([11](11-roadmap.md) critical path).
2. **Slice vertically.** One shell on one terminal working end to end, then the next OS. Do not parallelise adapters early.
3. **The engine knows nothing about the UI.** It returns candidates, match ranges and a replacement span ([06](06-completion-engine.md)). The UI only draws ([ADR-009](12-adr.md)).
4. **Never cut:** safe input handling, fail-open shell behaviour, clean uninstall, privacy, a usable inline fallback.

## Order of work

| Step | Scope | Backlog IDs | Done when |
|---|---|---|---|
| 0 | Decisions that block the repo: name, licence, v0.1 support table | [13](13-risks-open-questions.md) decisions 1–2 | Recorded in the repo README |
| 1 | Workspace + protocol types + JSON schema checks | IPC-01 (part), CORE-03 | `cargo test` validates `examples/*.json` against `schemas/*.json` |
| 2 | Tokenizer and context resolution, headless CLI | CORE-01, CORE-02 | `companion complete --buffer "git ch" --cursor 6 --json` prints the response in [`examples/completion-response.json`](../examples/completion-response.json) |
| 3 | Filesystem provider and escaping | CORE-04 | G04, G05, G10 pass |
| 4 | Daemon, socket, generation checks, fail-open | IPC-01, IPC-02 | AC-02 passes; killing the daemon mid-edit loses no keystroke |
| 5 | zsh adapter spike, then hardening | SPIKE-01, ADP-01 | Typing `git ch` in macOS zsh shows a JSON candidate; Tab edits the buffer safely |
| 6 | Inline renderer, keyboard flow | UI-01 | FR-04, FR-06 |
| 7 | OSC probe and theme resolver | SPIKE-05, UI-03 | U08, U09 |
| 8 | Bash, PowerShell, anchor and accessibility spikes | SPIKE-02/03/04/06 | Go/no-go recorded per spike |
| 9 | Native overlay | UI-02, UI-04 | U01–U12 pass in the overlay |
| 10 | Installers, security audit, QA matrix | INS-01, SEC-01, QA-01 | Release gates in [09](09-qa-test-plan.md) |

Steps 1–5 are **Increment A** in [14](14-backlog.md). Steps 6–9 are **Increment B**.

## Step 1–2: workspace and headless engine

Start with the crates from [10](10-devops-release.md) that Increment A needs; leave the rest out until used.

```text
crates/
├─ companion-protocol/   # IPC types (serde), version, size limits
├─ companion-spec/       # load + validate completion specs against schema v1
├─ companion-core/       # tokenizer, context resolution, ranking, replacement span
└─ companion-cli/        # `complete` (headless) first; install/doctor later
```

Rules for `companion-core`:

- All offsets in the protocol are **UTF-8 byte offsets**. Convert at the adapter boundary (zsh `CURSOR` counts characters; Windows APIs may count UTF-16 units) and validate that every offset is on a character boundary.
- Tokenise the text **before** the cursor for context, but keep the text after it so the replacement span never erases it (G07).
- Treat `\ ` as part of a token. An unterminated quote yields conservative or no suggestions and never modifies input (G05).
- No glob expansion, no variable substitution, no subprocess in the parser.
- Return `match_ranges` per candidate in byte offsets of the label.

Reproduce the prototype's behaviour as golden tests (see [17](17-design-system-handoff.md) for the file):

| Input | Expected first results |
|---|---|
| `gi` | `git` (command position) |
| `git ` | all `git` subcommands, spec order |
| `git ch` | `check-ignore`, `checkout`, `cherry-pick` (prefix), then in-order matches such as `fetch` |
| `git commit ` | options of `git commit` |
| `docker run -` | options of `docker run` |
| `npm run ` | scripts, `kind=argument` |
| `cd tai` | `Tài liệu/` (accent-folded match) |
| `cat My` | `My Projects/` inserted as `My\ Projects/` |
| `git checkout ` | branches only if the dynamic provider is enabled; otherwise paths |

## Step 4: daemon and fail-open

- One daemon per user, started on demand. Socket in the user runtime directory with a `0700` parent and `0600` socket; named pipe with a user ACL on Windows ([04](04-architecture.md)).
- Frames are length-prefixed JSON for the proof ([`schemas/ipc.schema.json`](../schemas/ipc.schema.json)). Reject frames over 16 KiB.
- Debounce 30–60 ms, cancel stale generations. An `accept` carries `generation` and `buffer_hash`; the daemon answers `stale` if either changed.
- The adapter must treat every daemon failure (missing socket, timeout, malformed reply) as "do nothing" and leave the shell's own completion intact (FR-07).

## Step 5: the zsh adapter spike

Question to answer: can a ZLE widget read `BUFFER` and `CURSOR` on each edit and apply an explicit edit without disturbing existing bindings (S-01)? The shape below is a sketch of what to try, not a tested adapter:

```zsh
# adapters/zsh/companion.zsh  (illustrative)
autoload -Uz add-zle-hook-widget

_companion_on_redraw() {
  # Send BUFFER (text) and CURSOR (characters) to the daemon.
  # CURSOR must be converted to a UTF-8 byte offset before it goes on the wire.
  # On any failure: return without touching the buffer.
}
add-zle-hook-widget zle-line-pre-redraw _companion_on_redraw

_companion_accept() {
  # Ask the daemon for the edit, verify generation and buffer hash,
  # then apply it in one step:
  #   BUFFER="${BUFFER[1,start]}${insert}${BUFFER[end+1,-1]}"; CURSOR=...
  # If no popup/suggestion is active, fall through to the user's original Tab widget.
}
zle -N _companion_accept
# bind Tab only after saving the existing binding; never remove it.
```

Checks for the spike: 10,000 random edit events produce a buffer identical to native zsh when no suggestion is accepted (NFR-04); history search, undo, bracketed paste and `vi` mode still work; password prompts and full-screen programs disable the adapter ([05](05-terminal-integration.md) state machine). If this cannot be done without intrusive bindings, record the decision and ship Tab-triggered completion instead.

## Step 6–7: inline renderer and theme

- Inline first. It needs no OS permission and is the accessible path. The adapter supplies `before`, `insertion`, `after`; the renderer draws ghost text and the `Tab` hint ([07](07-ux-design-system.md)).
- Implement the theme resolver as a pure function from `(terminal colours, osc status, user override)` to tokens plus a report of adjusted tokens. The prototype's `derive()` is the reference; port it with the same contrast rules (4.5:1 for text, muted text and matches on base and selected surfaces).
- Query OSC 10/11 only inside the adapter with a bounded timeout and a reply filter; never from the overlay.

## Step 9: native overlay

Run SPIKE-04 (anchoring) and SPIKE-06 (accessibility) before committing to egui/winit. Kill criteria are in [ADR-002](12-adr.md). The overlay must pass U01–U12 ([09](09-qa-test-plan.md)) with the same fixtures as inline.

## Per-step definition of done

Use the task template in [14](14-backlog.md). Each step ships with: unit and integration tests, the golden cases it claims, diagnostics updated (`doctor` reports capabilities per session), docs updated in the same pull request, and no raw command text in logs.

## Before the first commit

- Pin `rust-toolchain.toml`; add PR template, dependency review and licence scan (CI list in [10](10-devops-release.md)).
- Add `schemas/`, `examples/`, `design-system/` from this pack to the repo unchanged.
- Decide the product name; the repo may keep `terminal-companion` as codename.

## Do not

- Do not install a global key tap or read other apps' windows for command text ([ADR-006](12-adr.md)).
- Do not run completion-spec code or downloaded generators ([ADR-008](12-adr.md)).
- Do not draw an overlay when caret anchoring is unverified; fall back to inline.
- Do not advertise a terminal as supported until its matrix row is green ([05](05-terminal-integration.md)).
