# 12 — Architecture Decision Records (ADRs)

These are **proposed** decisions. Status moves from `Proposed` to `Accepted` only after R0 spikes and review. Record all reversals.

## ADR-001 — Rust core, shared across platforms

- **Status:** Accepted (2.1.0) · **Decision:** Rust for parser, IPC, spec loader, daemon, built-in providers.
- **Context:** cross-platform native binaries, low idle resource target, memory-safe core, no mandatory VM.
- **Trade-offs:** FFI/GUI and shell integration need OS-specific work; compile times; developer learning curve.
- **Alternatives:** Go (simple build, GC/runtime trade-offs); TypeScript/Node (ecosystem, heavier bundled runtime); Swift (mac-first, cross-platform friction).
- **Validation:** headless benchmark and cross-build pipeline.

## ADR-002 — Native overlay renderer candidate `egui` + `winit`

- **Status:** Accepted (2.1.0) · **Decision:** spike egui/winit first; keep UI independent of core.
- **Why:** portable Rust UI without browser-based runtime, supports theming and lightweight state.
- **Risks:** accessibility support, shaped/transparent no-activate windows, IME, OS-specific focus behavior, appearance fidelity.
- **Alternatives:** Tauri/WebView (easier HTML/CSS, browser runtime/dependency overhead); platform native SwiftUI/WinUI/GTK (best OS integration, 3 codebases); Slint (license/runtime evaluation needed).
- **Kill criteria:** cannot draw high-contrast keyboard-friendly popup reliably, or steals focus on required baseline.

## ADR-003 — Overlay is capability-based, inline fallback required

- **Status:** Accepted (2.1.0) · **Decision:** `display_mode=auto|overlay|inline`, default auto, session-specific capability probe.
- **Context:** Wayland and terminal accessibility APIs differ, and cannot be assumed universal.
- **Consequence:** fallback UI receives first-class testing; public support matrix declares mode, not just terminal name.

## ADR-004 — Separate own declarative spec schema from Fig runtime

- **Status:** Accepted (2.1.0) · **Decision:** static JSON schema v1, no JS runtime needed.
- **Why:** secure and predictable; portable; low latency; versioning simple.
- **Cost:** conversion needed for existing Fig TypeScript specs and runtime generators; no automatic drop-in compatibility.
- **Validation:** import five representative specs and compare supported static suggestions; unsupported fields clearly reported.

## ADR-005 — Command text stays in memory unless the user turns recording on

- **Status:** Accepted (2.1.0), amended · **Decision:** command buffers are ephemeral and never logged. Keeping the
  lines a user runs is opt-in (`privacy.recordCommands`, off by default) and goes to one private file (mode 0600)
  that `goh forget` deletes; lines that look like secrets or start with a space are never written. No telemetry.
- **Why amended:** finishing a line from history, next-command prediction and remembered option values need the
  record; making it opt-in keeps the original promise for everyone who does not ask for them.
- **Alternative rejected:** always-on usage analytics.

## ADR-006 — Integrate at shell editor, not global key event tap

- **Status:** Proposed · **Decision:** obtain state from shell itself and edit via native shell APIs; no system-wide keylogger.
- **Reason:** privacy, correctness, least privilege.
- **Open risk:** Bash event hooks insufficient for real-time suggestions; use Tab-triggered inline mode on failure.
- **Open:** `crates/goh-popup/src/scroll.rs` creates a macOS `CGEventTap` (scroll-wheel events only, no key events), so the check for "no event tap" does not come back empty; the record needs to say that the one tap is for scroll-wheel input only.

## ADR-007 — Theme is resolved per terminal session

- **Status:** Accepted (2.1.0) · **Decision:** prefer trusted session color source, fallback, user override; OSC 10/11 only with input-safe adapter.
- **Reason:** OS theme != terminal theme; unsafe OSC response handling pollutes prompt.
- **Cost:** some terminals provide approximate rather than pixel-perfect match.

## ADR-008 — Dynamic values come only from fixed, read-only built-in providers

- **Status:** Accepted (2.1.0), supersedes the opt-in proposal · **Decision:** specs cannot carry scripts or
  generators. Live values (git branches, Docker containers, kubectl pods, project scripts, ...) come from a fixed
  list of providers in the daemon, each running one program with fixed arguments (or reading a file), validating
  its output, bounded in time and size, and failing open. They are on by default because none of them runs
  anything a spec or the typed line supplies.
- **Why:** imported Fig generators can run arbitrary commands, leak secrets or hang; a fixed provider has none of
  those risks, so asking the user to opt in bought nothing.
- **Cost:** each new kind of value is a code change (`Provider` in `crates/goh-core/src/spec.rs`), not a spec edit.
- **Alternative rejected:** running Fig's TypeScript or shell generators, sandboxed or not.

## ADR-009 — The popup is presentational; the adapter owns selection, focus and insertion

- **Status:** Accepted (2.1.0) · **Decision:** the daemon/adapter owns the suggestion list, `selected_index`, window start, key handling and the text edit. The UI renders state and never takes focus or inserts text. Inline fallback receives its `insertion` from the adapter.
- **Why:** terminal input must stay with the shell (ADR-006); an insertion derived in the UI cannot know quoting, escaping or a cursor in the middle of a line.
- **Consequence:** the UI is replaceable (egui, WebView, shell-native inline) and testable with fixtures; the IPC carries match ranges and replacement spans ([06](06-completion-engine.md)).
- **Validation:** the same fixture states render identically in the overlay and inline renderers; U-cases in [09](09-qa-test-plan.md) pass.

## ADR-010 — `tokens.json` is the single source for visual values

- **Status:** Accepted (2.1.0) · **Decision:** colours, type, spacing, radii, sizes and shadows live in `design-system/tokens.json`. `tokens.css` and Rust constants are generated; nothing hard-codes a visual value.
- **Why:** the runtime theme resolver must override a small set of tokens and derive the rest; one source keeps overlay, inline and docs consistent.
- **Cost:** a generator step in CI; terminal-resolved colours bypass the file at runtime and must still satisfy the contrast rules.
- **Validation:** generated output matches the file; contrast check runs on all fallback themes and on derived palettes for the four test terminals.

## ADR-011 — The inline menu is drawn by ZLE, never by escape codes behind the shell

- **Status:** Accepted · **Decision:** where the native popup cannot draw, the zsh adapter frames the inline menu as a
  box in `POSTDISPLAY` with `region_highlight`, anchored at the word's column. goh does not proxy the PTY or write
  cursor-movement sequences of its own.
- **Why:** ZLE owns those lines, so clearing, scrolling, resizing, tmux and ssh stay correct with no terminal-specific
  code and no permission. A PTY proxy (the approach of ghost-complete) draws the same kind of floating box in the
  empty rows under the prompt, but ties goh to one platform, breaks over ssh and puts every byte of the session
  through goh.
- **Cost:** like any menu drawn in the terminal, the box uses the empty rows under the prompt; at the bottom of the
  screen the terminal scrolls up to make room. The column comes from measuring the prompt, so an unusual prompt
  shifts it.
- **Validation:** `zsh_pty.rs` box tests; manual check in Ghostty, Kitty, WezTerm and tmux.

## ADR template for future records

```markdown
# ADR-NNN — Title
Status: Proposed | Accepted | Deprecated | Superseded
Context:
Decision:
Alternatives:
Consequences:
Validation / rollback:
```
