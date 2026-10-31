# 12 — Architecture Decision Records (ADRs)

These are **proposed** decisions. Status moves from `Proposed` to `Accepted` only after R0 spikes and review. Record all reversals.

## ADR-001 — Rust core, shared across platforms

- **Status:** Proposed · **Decision:** Rust for parser, IPC, spec loader, daemon, built-in providers.
- **Context:** cross-platform native binaries, low idle resource target, memory-safe core, no mandatory VM.
- **Trade-offs:** FFI/GUI and shell integration need OS-specific work; compile times; developer learning curve.
- **Alternatives:** Go (simple build, GC/runtime trade-offs); TypeScript/Node (ecosystem, heavier bundled runtime); Swift (mac-first, cross-platform friction).
- **Validation:** headless benchmark and cross-build pipeline.

## ADR-002 — Native overlay renderer candidate `egui` + `winit`

- **Status:** Proposed · **Decision:** spike egui/winit first; keep UI independent of core.
- **Why:** portable Rust UI without browser-based runtime, supports theming and lightweight state.
- **Risks:** accessibility support, shaped/transparent no-activate windows, IME, OS-specific focus behavior, appearance fidelity.
- **Alternatives:** Tauri/WebView (easier HTML/CSS, browser runtime/dependency overhead); platform native SwiftUI/WinUI/GTK (best OS integration, 3 codebases); Slint (license/runtime evaluation needed).
- **Kill criteria:** cannot draw high-contrast keyboard-friendly popup reliably, or steals focus on required baseline.

## ADR-003 — Overlay is capability-based, inline fallback required

- **Status:** Proposed · **Decision:** `display_mode=auto|overlay|inline`, default auto, session-specific capability probe.
- **Context:** Wayland and terminal accessibility APIs differ, and cannot be assumed universal.
- **Consequence:** fallback UI receives first-class testing; public support matrix declares mode, not just terminal name.

## ADR-004 — Separate own declarative spec schema from Fig runtime

- **Status:** Proposed · **Decision:** static JSON schema v1, no JS runtime needed.
- **Why:** secure and predictable; portable; low latency; versioning simple.
- **Cost:** conversion needed for existing Fig TypeScript specs and runtime generators; no automatic drop-in compatibility.
- **Validation:** import five representative specs and compare supported static suggestions; unsupported fields clearly reported.

## ADR-005 — No command content persistence and telemetry opt-in

- **Status:** Proposed · **Decision:** command buffers ephemeral in memory, no raw logging/crash report.
- **Consequence:** harder debugging; compensate via synthetic repro harness and redacted status codes.
- **Alternative rejected:** always-on detailed usage analytics.

## ADR-006 — Integrate at shell editor, not global key event tap

- **Status:** Proposed · **Decision:** obtain state from shell itself and edit via native shell APIs; no system-wide keylogger.
- **Reason:** privacy, correctness, least privilege.
- **Open risk:** Bash event hooks insufficient for real-time suggestions; use Tab-triggered inline mode on failure.

## ADR-007 — Theme is resolved per terminal session

- **Status:** Proposed · **Decision:** prefer trusted session color source, fallback, user override; OSC 10/11 only with input-safe adapter.
- **Reason:** OS theme != terminal theme; unsafe OSC response handling pollutes prompt.
- **Cost:** some terminals provide approximate rather than pixel-perfect match.

## ADR-008 — Built-in safe dynamic providers only after opt-in

- **Status:** Proposed · **Decision:** default static+filesystem only; narrow read-only built-ins behind setting.
- **Reason:** imported arbitrary generators can run commands, leak secrets, or hang.
- **Alternative:** execute Fig TS generators directly (rejected for MVP).

## ADR-009 — The popup is presentational; the adapter owns selection, focus and insertion

- **Status:** Proposed · **Decision:** the daemon/adapter owns the suggestion list, `selected_index`, window start, key handling and the text edit. The UI renders state and never takes focus or inserts text. Inline fallback receives its `insertion` from the adapter.
- **Why:** terminal input must stay with the shell (ADR-006); an insertion derived in the UI cannot know quoting, escaping or a cursor in the middle of a line.
- **Consequence:** the UI is replaceable (egui, WebView, shell-native inline) and testable with fixtures; the IPC carries match ranges and replacement spans ([06](06-completion-engine.md)).
- **Validation:** the same fixture states render identically in the overlay and inline renderers; U-cases in [09](09-qa-test-plan.md) pass.

## ADR-010 — `tokens.json` is the single source for visual values

- **Status:** Proposed · **Decision:** colours, type, spacing, radii, sizes and shadows live in `design-system/tokens.json`. `tokens.css` and Rust constants are generated; nothing hard-codes a visual value.
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
