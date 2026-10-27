# 14 — Initial Engineering Backlog (epics / prioritized deliverables)

Effort is **relative rough order** (S/M/L/XL), not a commitment. Dependencies are task IDs. Mark items complete only with AC linked to docs 03 and 09.

| ID | Epic | Task / output | Priority | Size | Depends on | Acceptance |
|---|---|---|---|---|---|---|
| SPIKE-01 | Platform | ZLE event/edit prototype on macOS | P0 | M | — | S-01 pass |
| SPIKE-02 | Platform | Bash Readline feasibility + fallback choice | P0 | L | — | S-02 decision |
| SPIKE-03 | Platform | PSReadLine event/edit prototype | P0 | L | — | S-03 pass |
| SPIKE-04 | UI | macOS/Windows/X11 popup anchor experiments | P0 | L | — | S-04/05/06 report |
| SPIKE-05 | Theme | OSC 10/11 safe query and parser | P0 | M | — | S-07 report |
| CORE-01 | Core | UTF-8 cursor-aware shell tokenizer | P0 | L | SPIKE-01 | G01/G05/G07/G10 |
| CORE-02 | Core | Context tree resolution, static ranker | P0 | M | CORE-01 | G01/G02/G03 |
| CORE-03 | Specs | JSON schema loader, bundled Git/npm/Docker subset | P0 | M | CORE-02 | schema validation + golden |
| CORE-04 | Providers | Filesystem suggestions with escaping | P0 | M | CORE-01 | G04/G10 |
| IPC-01 | Runtime | Per-user socket/pipe, protocol, session manager | P0 | L | CORE-02 | SEC-01/NFR-08 |
| IPC-02 | Runtime | Generation checks, cancellation, failure-open | P0 | M | IPC-01 | AC-02/FR-07 |
| ADP-01 | Shells | zsh adapter + safe insertion | P0 | L | SPIKE-01, IPC-01 | M01/M02 |
| ADP-02 | Shells | bash adapter or conservative native completion | P0 | L | SPIKE-02, IPC-01 | M03/M04 |
| ADP-03 | Shells | pwsh7 adapter + PSReadLine compatibility | P0 | L | SPIKE-03, IPC-01 | M06 |
| UI-01 | UX | Inline fallback & keyboard behavior | P0 | L | ADP-01 | FR-04/06 |
| UI-02 | UX | Native overlay and no-focus positioning | P0 | XL | SPIKE-04, IPC-01 | verified mode-specific tests |
| UI-03 | Theme | Per-session resolver, contrast tokens | P0 | M | SPIKE-05, UI-01 | FR-05/AC-05 |
| CLI-01 | CLI | `doctor`, enable/disable, capability report | P0 | M | IPC-01 | FR-09 |
| INS-01 | Install | 3 OS installers + clean rollback | P0 | XL | ADP-01/02/03 | AC-06/NFR-06 |
| SEC-01 | Security | Privacy & IPC audit, malformed input fuzz | P0 | L | IPC-01, CORE-01 | SEC-01–06 |
| QA-01 | Test | E2E golden matrix, smoke, perf benchmarks | P0 | XL | UI-01, ADP-01/02/03 | NFR-01–10 |
| SPIKE-06 | UI | Accessibility: can the overlay expose option ids, active descendant and the status region to VoiceOver/Narrator/Orca? | P0 | M | SPIKE-04 | report; decide ADR-002 |
| DS-01 | Design | Generate `tokens.css` and Rust constants from `tokens.json` in CI | P0 | S | — | ADR-010 check green |
| DS-02 | Design | Render-check previews and prototype in a real browser; fix findings | P0 | S | — | R-12 closed |
| DS-03 | Design | Turn the prototype's completion, windowing and theme-derivation cases into golden fixtures | P0 | M | CORE-02 | U01–U12, G-cases |
| UI-04 | UX | Windowing, truncation, match ranges, permission notice in the real renderers | P0 | M | UI-01, UI-02 | FR-15/FR-16 |
| P1-01 | Shells | fish adapter | P1 | L | P0 release | fish test matrix |
| P1-02 | Specs | Static Fig-spec importer & attribution report | P1 | L | CORE-03 | 5 representative imports |
| P1-03 | Providers | Opt-in read-only built-in generators | P1 | M | SEC-01 | FR-11 |
| P1-04 | Product | Local spec overrides and config UI | P1 | M | CORE-03 | FR-10/12 |

## First two implementation increments

**Increment A (proof, no GUI):** SPIKE-01, CORE-01/02/03, IPC-01, ADP-01. A user types `git ch` in macOS zsh and sees headless JSON candidate; Tab edits safely. Instrument latency and fail-open behavior.

**Increment B (real MVP skeleton):** SPIKE-02/03/04/05/06, ADP-02/03, UI-01/03/04, CLI-01, DS-01/02/03. Prove cross-platform viability before polishing the overlay.

## Task template

```markdown
Title:
Why / User story:
Dependencies:
Scope:
Non-goals:
Acceptance criteria (requirement IDs):
Tests & diagnostics:
Security/theme/accessibility impact:
Definition of Done:
```
