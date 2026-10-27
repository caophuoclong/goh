# 11 — Delivery Roadmap & Milestones

**These are relative phases, not promises or calendar deadlines.** Start with feasibility gates rather than polished UI.

| Phase | Focus | Deliverable | Exit gate |
|---|---|---|---|
| R0 — Discovery (1–2 sprints target) | Shell input feasibility, caret anchoring, theme probe | 7 spikes, compatibility notes, architecture decision | Each platform has a safe path, native or fallback |
| R1 — Engine (2–3 sprints) | parser, schemas, static spec, local IPC, CLI | headless completion CLI + golden tests | Git/npm/docker static cases pass |
| R2 — Shell adapters (2–4 sprints) | zsh, bash, pwsh7, safe insertion | adapter POCs hardened + diagnostics | no input regression on baseline pairs |
| R3 — UI & theme (2–3 sprints) | overlay/inline, auto palette, keyboard flow, UI contract ([07](07-ux-design-system.md)) | visual functional beta; design system and prototype ([17](17-design-system-handoff.md)) already delivered as the visual reference | theme + cross-display gate green, U01–U12 pass |
| R4 — Security & performance (1–2 sprints) | audit, packaging, benchmark | signed RC packages, privacy test | all P0 AC and release gates pass |
| R5 — Stabilization (ongoing) | bugfix, documentation, wider terminals | v0.1 beta / v0.2 | issue trends and retention acceptable |
| P1 | fish, more terminals, local overrides, safe dynamic providers | v0.3+ | planned separately |
| P2 | SSH deep support, plugins, spec registry, AI optional | TBD | privacy/complexity review first |

## Critical path

**R0 capability spikes → shell editor/IPC contract → safe insertion & fallback → suggestion engine → UI positioning/theme → real-terminal E2E → release**.

Reason: a beautiful popup is worthless if reading edits breaks Readline/PSReadLine or if Wayland refuses global anchors.

## Suggested squad ownership

| Workstream | Responsibility |
|---|---|
| Runtime | Rust core, parser, schema, IPC, providers |
| Platform | zsh/bash/pwsh adapters, caret APIs, installer |
| UX | popup, fallback UX, tokens, accessibility |
| QA/Security | input regression, test matrix, privacy, release gate |
| Product/Lead | prioritization, support promise, telemetry posture |

For a solo developer, follow the same sequence but slice work vertically: one shell+terminal working, then another OS; don't parallelize prematurely.

## Scope cuts if schedule slips

1. **Never cut** safe input handling, failure-open shell behavior, uninstall integrity, privacy, or usable fallback.
2. Cut rich icons, animation, fuzzy ranking, tool-specific dynamic generators, broad spec catalog.
3. Limit claimed supported terminal combinations to those verified rather than shipping a misleading support badge.
4. Prefer completion triggered by Tab on Bash/Wayland if on-every-key integration is unreliable.

## Definition of version numbers

- `v0.1-alpha`: engine + zsh prototype, not for normal daily work.
- `v0.1-beta`: user-facing cross-platform baseline with each OS's declared UI mode, known limitations.
- `v0.2-stable`: hardening, performance regression guard, clean installers, wider spec coverage.

Timeline and names should be revisited after R0.
