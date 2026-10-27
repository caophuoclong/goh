# 09 — QA Strategy, Test Matrix & Benchmark Plan

## Testing pyramid

- **Unit:** parser/tokenizer, shell escaping, UTF-8 ranges, schema validation, theme palette parser, ranker, permission policy.
- **Property-based / fuzz:** malformed shell inputs, OSC replies, IPC framing, nested specs, unicode boundaries, partial quotes; no panic/hang.
- **Integration:** shell adapter ↔ daemon, per-user IPC, session isolation, test dynamic generator timeout, uninstall rc file preservation.
- **E2E:** synthetic typed command sessions in terminal with real key events; image/geometry checks for overlay, inline fallback, accessibility.
- **Manual exploratory:** multiple monitors, DPI, shell customizations, terminal theme switching, bracketed paste, IME, tmux/ssh exclusions.

## Target compatibility matrix

| Test ID | OS & shell | Terminal | Backend | Release gate |
|---|---|---|---|---|
| M01 | macOS zsh | Terminal.app | native | Required, overlay or declared fallback |
| M02 | macOS zsh | iTerm2 | native | Required, overlay or declared fallback |
| M03 | Ubuntu LTS bash | GNOME Terminal | X11 | Required, overlay or declared fallback |
| M04 | Ubuntu LTS bash | GNOME Terminal | Wayland | Required, **inline fallback** |
| M05 | Ubuntu LTS bash | Kitty | X11/Wayland | Smoke baseline, display mode documented |
| M06 | Windows 11 PowerShell 7 | Windows Terminal | Win32/UIA | Required, overlay or declared fallback |
| M07 | macOS/Linux tmux, SSH | Terminal baseline | multiplexer/remote | Non-regression & limitation documented |
| M08 | Integrated VS Code terminal | supported OS shell | embedded | P1/experimental, don't advertise full support yet |

Pin specific OS, shell, PSReadLine, terminal and compositor versions in each release test artifact. Above list describes **test commitments**, not a statement that support already exists.

## Golden test suite (examples)

| ID | Input | Expected assertion |
|---|---|---|
| G01 | `git ch` | suggests checkout, replacing `ch`, buffer remains editable |
| G02 | `git commit -` | suggests relevant flags, no arg insertion before selection |
| G03 | `docker ru` | suggests run if bundled spec enabled |
| G04 | `cd My` | directory `My Folder` inserted with safe escaping |
| G05 | `cd 'My` | incomplete quote handled conservatively |
| G06 | `git --help` | no spurious executable side effect |
| G07 | Cursor in middle of `git chekout` | replacement span does not delete suffix unexpectedly |
| G08 | `echo password=abc` | no persistence to logs/metrics |
| G09 | Paste 20 KB | adapter bypasses or safely rejects oversized IPC request |
| G10 | Unicode `đường-dẫn` | valid UTF-8 ranges and screen width |
| G11 | Rapid typing with stale gen | old suggestions never inserted |
| G12 | Screen alternate mode (vim/less) | popup remains hidden |

## UI conformance cases

Each case is exercised against the fixtures in [17](17-design-system-handoff.md) in both the overlay and inline renderers where it applies.

| ID | Case | Expected assertion |
|---|---|---|
| U01 | 12 results, select the 11th | Selected row visible; window moves one row at a time; footer shows `11/12` |
| U02 | Down on last, Up on first | Selection wraps; window jumps with it |
| U03 | Label longer than the popup | Description disappears first; label ends with an ellipsis; full label in accessible name and tooltip |
| U04 | Less than 320 px available | Popup not drawn; inline fallback used |
| U05 | CJK label and label with combining marks | Ranges land on grapheme boundaries; no split mark; width not clipped |
| U06 | Fuzzy match, e.g. `clean` for `cn` | Only the matched characters are highlighted, from that item's own ranges |
| U07 | Light container nested in a dark theme | `text-match`, `focus-ring` and selected marker use the light accent; contrast ≥ 4.5:1 |
| U08 | Low-contrast terminal palette | Derived colours are adjusted; diagnostics name the adjusted tokens |
| U09 | OSC reply: both / background only / none | Theme source reads verified / approximate / fallback; prompt stays clean |
| U10 | Inline: quoted path, cursor mid-line | Insertion comes from the adapter; text after the cursor is unchanged; nothing drawn without a valid insertion |
| U11 | Permission denied | Single notice with recovery step; inline suggestions still work; not repeated per prompt |
| U12 | Screen reader | Selection announced once per change with position; no announcement per keystroke; terminal keeps focus |

## Theme tests

Use terminal profiles with known fg/bg: dark charcoal, bright paper, low-contrast purple, high-contrast monochrome. Validate color source resolution and UI readable contrast. Test OSC response with BEL/ST terminators, 4-/8-/16-bit color formats, response timeout, mux passthrough, and malformed payloads. Verify no escape bytes leak to editable command buffer.

## Non-functional benchmarks

**Reference hardware must be recorded before release:** e.g. one 8 GB macOS laptop, one 8 GB Ubuntu VM/native system, one 8 GB Windows 11 PC; OS/version/CPU/architecture and thermal state attached to report.

- **Latency:** synthetic 10k static suggestion requests, warm/cold cache split, record p50/p95/p99 from IPC receive to response output. Target p95 ≤ 50 ms, p99 ≤ 120 ms warm static; report cold separately.
- **Shell startup:** 100 launches per shell with hook enabled/disabled, run under identical configuration, calculate median/p95 overhead. Target p95 overhead ≤ 15 ms.
- **Memory & CPU:** 10-minute idle measurement and 5-minute active burst, report daemon RSS, UI RSS, process count. Daemon target ≤ 60 MB RSS idle, < 1% CPU steady.
- **Input safety:** randomized 10k event sequence per supported shell (edits, delete, history, paste, arrows, undo). Golden final buffer MUST equal reference native shell result for non-acceptance operations.
- **Robustness:** kill daemon/UI mid-edit 100 times, restart service, ensure shell stays usable. Test multiple sessions/cwd isolation.

## Test automation in CI

- Every PR: formatting/lint, unit/property tests, JSON schema validation, license/dependency scan.
- Nightly per OS: integration, shell startup, non-flaky headless terminal simulation; Windows shell tests.
- Release candidate: manual real-terminal matrix, perf baselines, accessibility smoke, signed artifact/install/uninstall.
- Flaky E2E should not be hidden: label quarantine, bug owner, and blocking rules recorded.

## Exit criteria

0 P0 open blockers, 0 dropped/double typed characters across gate corpus, no known critical data leakage, installer rollback pass, matrix mode accurately documented, no untriaged crash, accessibility and theme review complete.
