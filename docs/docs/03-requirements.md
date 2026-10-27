# 03 — Software Requirements Specification (SRS) & Acceptance Criteria

**ID taxonomy:** `FR-*` functional, `NFR-*` non-functional, `SEC-*` security. P0 MUST; P1 SHOULD; P2 MAY.

## Functional requirements

| ID | Priority | Requirement | Testable acceptance criteria |
|---|---|---|---|
| FR-01 | P0 | Recognize active editable command line and cursor position from supported shell | For 3 baseline shell/platform pairs, key input at prompt produces valid request; password/fullscreen disabled |
| FR-02 | P0 | Suggest command/subcommands/options from spec with prefix match | `git ch` returns `checkout`; `git commit -` returns flags; ordering deterministic for same input |
| FR-03 | P0 | Complete local files/directories respecting cwd and quoting | Path with space/unicode hidden file passes golden tests; no access outside current user privilege |
| FR-04 | P0 | Keyboard navigation and safe insertion | Tab accepts; up/down move highlight; Esc hides; Enter defaults to shell; no execution on Tab |
| FR-05 | P0 | Theme reflects terminal palette where detectable | fg/bg sampled through supported adapter/OSC query; override persists; unknown source falls back sanely |
| FR-06 | P0 | Inline fallback when overlay cannot be positioned | Wayland test has usable suggestions or native shell uninterrupted; no UI detached from caret |
| FR-07 | P0 | App failure never blocks native editing | Kill daemon while typing -> keystrokes kept, shell remains functional |
| FR-08 | P0 | Install, enable, disable, uninstall shell integration | `install --dry-run` shows diff; idempotent install; uninstall preserves unrelated rc content |
| FR-09 | P0 | Diagnostics without secret leakage | `doctor --json` includes compatibility/permission checks but no command buffers or env secrets |
| FR-10 | P1 | Local user completion specs override built-in packs | Schema validation and precedence tested; invalid spec emits safe diagnostic |
| FR-15 | P0 | Suggestion window follows selection and handles long content | 12 results: 11th selected stays visible, selection wraps, `n/total` shown; long labels truncate with full text available; popup not drawn below 320 px (U01–U04) |
| FR-16 | P0 | Match highlighting, permission notice and inline insertion follow the UI contract | Per-item match ranges on grapheme boundaries; one permission line with recovery step; inline insertion supplied by the adapter, cursor-in-middle safe (U05, U06, U10, U11) |
| FR-11 | P1 | Optional dynamic suggestions with permission | Generators disabled by default; strict argv allowlist, timeout, cancellation |
| FR-12 | P1 | Persist lightweight per-terminal preferences | Options overrideable global → OS → terminal → profile → session (documented resolution) |
| FR-13 | P1 | Operate offline and without account | All static suggestions work without network |
| FR-14 | P1 | Fuzzy/personalized ranking (local-only) | Optional setting, deterministic replay when toggled off |

## Non-functional requirements

| ID | Target / acceptance |
|---|---|
| NFR-01 | Static-engine processing time p95 ≤ 50 ms, p99 ≤ 120 ms measured from message receipt to result produced, benchmark scenario defined in QA doc |
| NFR-02 | Daemon steady-state RSS target ≤ 60 MB, idle CPU < 1% on defined hardware; UI peaks measured and published separately |
| NFR-03 | Shell adapter adds p95 ≤ 15 ms to interactive shell launch on tested baseline, not counting user existing init |
| NFR-04 | No dropped, reordered, or double-inserted input in >10,000-event stress test, including rapid text edits |
| NFR-05 | Compatible binaries shipped for specified macOS/Linux/Windows architectures; supported combo table maintained each release |
| NFR-06 | Install/uninstall restores exact unrelated rc/profile content; rollback test with customized profiles |
| NFR-07 | UI keyboard-only usable; visible focus; text contrast aims WCAG 2.2 AA for body text, with terminal color adjustments where needed |
| NFR-08 | Single daemon handles multiple concurrent terminal sessions without cross-session bleed |
| NFR-09 | No cloud dependency for normal suggestion flow; permission failures degrade safely |
| NFR-10 | Config/schema backward-compatible for 1 minor release; data version migration test |
| NFR-11 | Popup never takes focus; selection announced once per change (throttled), never per keystroke; fallback palettes and derived palettes keep text, muted text and matches ≥ 4.5:1 on base and selected surfaces (U07, U08, U12) |

## Security requirements

| ID | Requirement | Acceptance |
|---|---|---|
| SEC-01 | Per-user IPC and local authorization | Other OS user cannot read/write request pipe/socket in integration test |
| SEC-02 | No shell code evaluation of user command text | Input treated as data; parse-only and range-edit only |
| SEC-03 | No unsolicited network transfer | Network capture offline test shows 0 requests for normal operations |
| SEC-04 | Dangerous generators cannot run silently | Default disabled except documented read-only built-ins; action prompts for expanded permissions |
| SEC-05 | Sensitive fields excluded from logs | Automated redaction fixtures for token/password patterns and no raw command text stored |
| SEC-06 | Signed/verifiable releases where platform supports | Build provenance/checksum and release identity verified in pipeline |

## Given/When/Then acceptance examples

**AC-01 — Prefix suggestion**

- Given zsh integration is active and `git` spec loaded; when user types `git ch`; then first suggestions include `checkout` without executing a command.

**AC-02 — Stale event**

- Given generation ID `41` from request `git ch`; when user types more text producing generation `42` before response `41`; then result `41` must not be shown nor inserted.

**AC-03 — Quoted argument**

- Given cwd contains `My Folder`; when user types `cd My` and chooses result; then shell buffer must be a syntactically valid path with appropriate quoting/escaping and cursor at expected location.

**AC-04 — No overlay**

- Given Linux Wayland compositor denies global coordinate access; when user requests completion; then inline mode works or native shell behavior remains intact, and app never misplaces global overlay.

**AC-05 — Exact-theme source absent**

- Given OSC queries unsupported/timed out; when theme mode = `auto`; then UI uses neutral fallback or configured override without hanging the prompt.

**AC-06 — Disable/uninstall**

- Given customized `.zshrc` before installation; when user uninstalls; then only tool's bounded marker block is removed; hash/diff of all unrelated lines is unchanged.

## Definition of done (per P0 feature)

Implemented, unit+integration tests passing, shell regression tests, docs and diagnostics updated, security/privacy checked, and acceptance demo recorded **without real secrets**. Unsupported combinations documented.
