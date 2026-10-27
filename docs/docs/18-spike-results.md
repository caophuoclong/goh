# 18 — Spike results (implementation steps 8–9)

> **Note on names.** This record was written while the project was still called `fig`. When it was renamed to `goh`, the names below were updated mechanically (`goh-*` crates and binaries, `gohd`, `GOH_*` variables, `~/.config/goh/`), including in the historical passages: for example, the daemon and popup that an over-broad `pkill` killed at the time were then called `figd` and `fig-popup`. The probe data under `docs/spikes/s04/` is untouched.

**Status:** partial. Records what was actually run, what was not, and the decision each result supports. "Not run" means no result exists, not that the answer is positive.

## Summary

| Spike | Ran? | Evidence | Decision |
|---|---|---|---|
| S-02 Bash Readline | Partly | `adapters/bash/goh.bash`, 4 pty tests passing on **bash 3.2.57 (macOS)** and **bash 5.2.15 (Debian 12, Linux arm64, Docker)** | **GO** for a completion-function adapter (the fallback ADP-02 names). **NO-GO** for per-keystroke `bind -x` capture |
| S-03 PowerShell / PSReadLine | No | No `pwsh` on the test machine | Open. One blocker already visible from the code: see below |
| S-04 macOS anchor | Partly. **iTerm2 (1 pane, 1 screen): PASS. Terminal.app (split lower pane; single pane on the main screen and on a second screen): PASS (corroborated).** Probe data in `docs/spikes/s04/` | 48/48 positions on the cell grid in every Terminal.app run and in iTerm2 (iTerm2 needs the tall-rect rule for 3 line-start cases); on a second screen all 48 rectangles moved by exactly the window's move (+1601, +266 pt) | **GO for iTerm2 and Terminal.app in the configurations measured**; mixed-scale screens, iTerm2 split/second screen, live prompt, resize/scroll/IME/zoom/full screen open |
| S-05 Windows anchor | No | No Windows machine | Open |
| S-06 Linux X11 / Wayland anchor | No | No Linux desktop | Open |
| SPIKE-06 Accessibility | No | No screen reader session | Open |
| ADR-002 egui/winit renderer | Partly | `goh-popup` draws the design (screenshots), model has unit tests for U01–U06, U12; focus check passed on macOS (iTerm2) | **GO** for drawing, layout and not becoming the frontmost app on macOS; Windows/Linux and keyboard delivery unverified (see Step 9) |

Consequence for the first release: **inline is the only display mode with any evidence behind it**. The overlay stays **NO-GO** until S-04/05/06 produce numbers, as [ADR-003](12-adr.md) and [05](05-terminal-integration.md) already require ("do not draw an overlay when caret anchoring is unverified").

## S-02 Bash

**Question:** low-overhead reliable capture, or an explicit fallback?

**Result:** use bash's programmable completion. The adapter registers `complete -F _goh_complete` for commands that have a spec and touches no key binding.

- Why not `bind -x`: from the bash documentation (not measured here), `READLINE_LINE`/`READLINE_POINT` exist only from bash 4.0, and a `bind -x` handler cannot call readline functions, so it cannot fall through to native completion on failure. That breaks FR-07 by construction. macOS ships bash 3.2, so a `bind -x` design would not work on the default macOS bash at all.
- Fail-open is the default behaviour: no candidates means `-o default` returns the word to bash's filename completion. Tested: `git zzf<Tab>` completes the file, with and without a running daemon.
- Verified on bash 3.2.57 and on bash 5.2.15 (pty, real Tab): unique candidate inserted; ambiguous candidates listed by bash's own list; text with non-ASCII before the word; cold start (no daemon) leaves the line intact and starts the daemon; `goh-unload` removes only its own registrations.

**Limits (by design):** only commands with a spec (no command-word completion); candidates must extend the typed word, so no fuzzy matching; no ghost text or menu of our own; with non-ASCII text and the cursor not at the end of the line it offers nothing, because `COMP_POINT` is not portable (measured below).

**Measured: `COMP_POINT` units.** With the line `xx ééé b` and the cursor after `xx ééé` (6 characters, 9 bytes), a completion function saw `COMP_POINT=9` on bash 3.2.57 (bytes) and `COMP_POINT=6` on bash 5.2.15 (characters). Bash 4.x and 5.0/5.1 were not measured, so the adapter does not assume either.

**Also learned from the Linux run:** `goh-core`, `goh-daemon` (socket, termios, process spawning) and `goh-ctl` build and run on Linux arm64 with no changes. The zsh tests were not run there (no zsh in the image).

**Not verified:** Ubuntu with GNOME Terminal or Kitty (the S-02 matrix names them; the container ran under a pty, not a real terminal emulator), bash 4.x, x86_64 Linux, `bash-completion` coexistence beyond "an existing spec is left alone", performance.

## S-03 PowerShell

Not run. Findings that do not need a Windows machine:

1. **Transport.** `goh-daemon` uses `std::os::unix::net`, which does not exist on Windows. The IPC layer needs a named-pipe transport (as [04](04-architecture.md) already says) or a Unix-socket crate before any Windows work. The frame format and server logic are transport-independent, but `paths.rs`, `server.rs`, `client.rs` and `probe.rs` are not.
2. **Offsets.** PSReadLine's cursor counts UTF-16 units; the wire uses UTF-8 bytes. The adapter must convert, as the zsh adapter does with `LC_ALL=C`.
3. **Shape of the adapter** (illustrative, untested): `Set-PSReadLineKeyHandler -Chord Tab`, read the line with `[Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState`, apply with `Replace`/`SetCursorPosition`, and call `TabCompleteNext()` on any failure.

## S-04 macOS anchor: how to run it

**Question:** can the Accessibility API give a caret rectangle that is right ≥ 98% of the time (go criterion in [05](05-terminal-integration.md))?

**Probe:** `crates/goh-popup/tools/anchor-probe.swift`. Run it in the terminal under test, at a shell prompt, with that terminal enabled under System Settings > Privacy & Security > Accessibility:

```bash
swift crates/goh-popup/tools/anchor-probe.swift --note "iTerm2, single pane, 1 monitor" --out iterm-single.json
```

It checks that the focused element exposes `AXSelectedTextRange` and `AXBoundsForRange`; asks the terminal for its cell size and grid (`16t`/`18t`, and iTerm2's `ReportCellSize`); switches to the alternate screen, writes known text, and places the cursor **on characters** (in-text) and **right after typed text** (end-of-text, what a prompt does) with escape sequences; reads the caret rectangle after each move; and judges each against where the cell grid says it must be (within half a cell, inside the window, non-empty). Positions with no rectangle count as wrong. Both groups must reach 98%. It restores the screen and cursor, and records the app, macOS version, screens and scales, focused role and every attribute name (useful for SPIKE-06). Without a terminal-reported cell size it returns INCONCLUSIVE, never PASS, because it would only be comparing the API with itself.

### First run: iTerm2 3.7.3, macOS 27.0.1, one pane, one Retina monitor (probe v1)

Raw data: `docs/spikes/s04/iterm2-single-pane-v1-blank-cells.json`. The probe reported FAIL (15.8%), but **that corpus was wrong**, so the number is not a verdict on iTerm2. What the data does show:

- The focused element is an `AXTextArea` with `AXSelectedTextRange`, `AXBoundsForRange`, `AXInsertionPointLineNumber` and `AXVisibleCharacterRange`.
- The selection **follows the terminal cursor**: on one row, moving from column 5 to 20 to 83 to 161 changed the text index by exactly +15, +63, +78.
- Where the line has text under the cursor, the rectangle is **exactly on the cell grid**: column 5 is at x=41 on rows 1, 23 and 44, and y = 68, 486, 885 = 68 + 19 × (row − 1). Cell size 9 × 19 points (18 × 38 px at scale 2); 20 × 9 = column 20 at x=176 on the same rows.
- Where the cursor sat in blank cells, results were wrong (y jumped to later lines). The text the API exposes drops trailing blanks, so an index past a line's last character lands on a following line. Blank cells are not a state a prompt produces, which is why v1 measured the wrong thing; v2 only places the cursor in text or immediately after it.
- iTerm2 did not answer `16t`, so v1 had no independent cell size. v2 also asks `OSC 1337 ; ReportCellSize`.
- `AXBoundsForRange` with length 1 on a newline returns a rectangle as wide as the whole line; v2 prefers length 0 and records which call produced each rectangle.

### Second run (probe v2), same setup

Raw data: `docs/spikes/s04/iterm2-single-pane-v2-bad-reference.json`. The probe printed FAIL (in-text 10.5%, end-of-text 0%). **That verdict was a bug in the probe**: it used the first sample (row 1, column 1) as the reference, and that one sample is an outlier (its rectangle starts one line too high), so every expected position was off by one line. Re-judged offline from the same file, with the grid origin taken as the median of the one-cell-tall rectangles:

- **29 of 29 one-cell-tall rectangles are exactly on the grid, 0 pt error** (origin x=5, y=68; cell 9 × 19 pt, confirmed independently by iTerm2's `ReportCellSize` = 19;9;scale 2).
- **End-of-text (cursor right after typed text, rows 1, 2, 23, 44): 12/12 exact.**
- **In text: 17/20 strictly; 20/20 with a "tall-rect rule".** The 3 exceptions are all column 1 of a line whose previous line is empty (rows 1, 23, 44): the API returns a rectangle two lines tall that starts on the line above. The caret is the last line of that rectangle (`y + h − cell height`). Column 1 of row 2 (previous line has text) was exact. A product anchor needs this rule.
- Still not covered: the **last rows of the screen** (the bottom prompt, where no line follows). Probe v3 adds rows − 1 and rows. Also not yet tested in a live prompt, only in the alternate screen with scripted text.

### Third run (probe v3, corrected judge), same setup: **PASS**

Raw data: `docs/spikes/s04/iterm2-single-pane-v3.json`. Probe v3 takes the grid origin from the median of all one-cell-tall rectangles (x=5, y=68; cell 9 × 19 pt from `ReportCellSize`), reports strict and tall-rect-rule percentages separately, and adds the last two screen rows.

| Group | Strict | With tall-rect rule |
|---|---|---|
| End-of-text (cursor right after typed text; rows 1, 2, 23, 44, **45, 46**) | 18/18 (100%) | 18/18 |
| In text (rows 1, 2, 23, 44, 45, 46 × 5 columns) | 27/30 (90%) | 30/30 (100%) |

- The **last screen row** (46, no line after it) is exact: x = 5 + (col − 1) × 9, y = 923 = 68 + 45 × 19, for end-of-text and in-text positions.
- The 3 strict misses are the same case as in v2 (column 1 of rows 1, 23, 44, where the previous line is empty). The rule that fixes it, "if the rectangle is two lines tall, the caret is its last line", was derived from the v2 data and held on v3 with the same 3 positions, so it is reproducible, but it is a rule the product must implement.
- Every one-cell-tall rectangle (45 of 48) had **0 pt** error. Rectangles are in screen points with a top-left origin, the same unit as the window frame.
- Weak spots in the probe itself: `grid_origin_inside_text_area` is nearly vacuous because iTerm2's text area includes the whole scrollback (its frame is 8555 pt tall); and the caret rule is checked against the same grid, so only the cell size is independent (it is, from iTerm2).

### Terminal.app 2.15, macOS 27.0.1 (probe v3)

Raw data: `docs/spikes/s04/terminalapp-split-lower-pane-v3.json`.

- The focused element is an `AXTextArea` with `AXSelectedTextRange` and `AXBoundsForRange`, like iTerm2.
- **The rectangles behave differently from iTerm2's**, which matters for the implementation: a zero-length range returns an empty rectangle, so every sample came from the **one-character** range (`bounds_call = length1`). At a position with text that is the character's cell (7 × 15 pt). At the end of typed text the character is the line break, and the rectangle is as wide as the rest of the line (420–819 pt), but its **top-left corner is exact**. **An anchor must use a rectangle's top-left and the cell height, never its width.** No two-line-tall rectangles appeared (0 of 48).
- **48/48 positions are on one grid**: origin (50, 341), cell 7 × 15 pt, in text and after typed text, strict and with the rule alike.
- **The terminal did not report a cell size** (no `16t`, no `ReportCellSize`), so probe v3 printed INCONCLUSIVE, correctly by its own rules. Evidence that stands in for it, from the same data: the single-character box (7 × 15) equals the pitch between positions; `(window width − 20) / columns = (860 − 20) / 120 = 7.0`; and the grid sits in the window with insets of **10 pt on the left, right and bottom** (top 288 pt, which is the other pane). A grid that fills the window minus equal margins is hard to produce by accident, and it shows the rectangles are in the right place in screen coordinates for the focused pane, not pane-relative. This is corroboration, not an independent measurement. Probe v4 (typechecked, not yet run) reports these checks and returns "PASS (corroborated)" when they hold; re-judged offline, this file would get that.
- **Split panes:** the probe measured the focused (lower) pane (grid 120 × 15 in a window about 31 rows tall). Which pane was focused for the other runs is not recorded, and upper panes, side-by-side splits and single-pane Terminal.app were not run.

### "Second monitor" run, first attempt: not a second monitor

`terminalapp-monitor2-NOT-a-second-screen-v3.json` differed from the split run only in `text_index` and the note: same window frame, same grid origin, every rectangle identical, and `screens` listed one screen. The window had not been moved (or the display was mirrored). Probe v4 records `screen_count` and the grid's insets in the window so this is visible in the output.

### Terminal.app 2.15, single pane, main screen and second screen (probe v4): PASS (corroborated)

Raw data: `terminalapp-single-pane-main-screen-v4.json` and `terminalapp-single-pane-second-screen-v4.json`; both 120 × 30 grid, 7 × 15 pt cell.

- **Main screen** (`screen_count` 1): 48/48 positions (30 in text, 18 after typed text), origin (50, 92).
- **Second screen** (`screen_count` 2; second screen frame x 1512, y −18, 1600 × 1000, scale 2; window at x 1641, y 319): 48/48 positions, origin (1651, 358).
- **Independent check of absolute position:** between the two runs the window moved by (+1601, +266) pt (from its own Accessibility frame) and **all 48 rectangles moved by exactly (+1601, +266) pt**; the grid keeps the same insets in the window (10 left, 10 right, 10 bottom, 39 top) on both screens. So the coordinates are global screen points that follow the window across displays, not per-screen or per-pane values.
- Both runs: the single-character box (7 × 15) equals the pitch, so probe v4 reports "PASS (corroborated)", not an independent cell-size confirmation, because Terminal.app does not report its cell size.
- **The mixed-scale case was not exercised:** both screens report `scale` 2. The second display was described as non-Retina, but macOS reports scale 2 for an external panel running in a scaled (HiDPI) mode, so a true scale-1 display was not tested. Accessibility rectangles are in points either way; whether anything changes for a scale-1 screen is unknown.
- For the popup: the second screen's `NSScreen` frame has a bottom-left origin (y −18, height 1000), while Accessibility rectangles use a top-left origin (screen top at y 0). Converting between them, and choosing the screen that contains the caret, is needed for flipping the popup near screen edges; it is not implemented or tested.

**What S-04 establishes (overall):** in iTerm2 3.7.3 and Terminal.app 2.15 on macOS 27.0.1, for scripted text on the alternate screen, the Accessibility API gives a caret rectangle whose top-left corner is on the cell grid in every one of 48 positions per run, in global screen points, in the focused pane. **Per-app rules an anchor must implement:** iTerm2 uses a zero-length range and needs the tall-rect rule at column 1 after an empty line; Terminal.app needs the one-character range, and only the rectangle's top-left and the cell height may be used (its width is wrong at the end of a line). **Not established:** a live zsh/bash prompt (prompt themes, wrapped lines, redraws), mixed-scale or scale-1 screens, Terminal.app side-by-side or upper panes, iTerm2 on a second screen or in split panes, resize, scroll, IME, font zoom, window move while the popup is shown, full screen, other app versions. A verified anchor can be claimed for those two apps in the configurations above, not for "macOS".

**Matrix to run:** iTerm2 and Terminal.app × single pane and split panes × window on each of two monitors (and a Retina and a non-Retina display if available). Not covered by the probe and still to do by hand: resize, scroll, IME, font zoom, moving the window, full screen.

**Not run yet.** The probe was written and typechecked here, but the sandbox has no Accessibility access, so none of its Accessibility logic has been exercised.

## S-05/06 and SPIKE-06

Not run. What each needs: S-05 Windows Terminal and VS Code with UI Automation; S-06 GNOME on Wayland and X11. SPIKE-06 needs VoiceOver, Narrator and Orca sessions against a real overlay.

## Step 9 — native overlay (partial)

**Built** (`crates/goh-popup`):

- `model`: the presentational logic as pure functions with tests: windowing (one row at a time, wrap jumps the window), `n/total` footer, width 320–520 px, description-then-label truncation, grapheme-safe match ranges, CJK counted in columns, option ids and a status string that changes only with the selection. Covers **U01, U02, U03, U04, U05, U06, U12** at the model level.
- `display`: the ADR-003 gate. The overlay is chosen only when the anchor is **verified**, focus is preserved and at least 320 px are available; a forced `overlay` setting does not bypass it. Reasons are returned for diagnostics.
- `goh-popup` binary: egui/eframe 0.37 (glow), borderless, transparent, always-on-top, mouse-passthrough, hidden until a `show` frame, `UIElement`/accessory activation policy on macOS. It reads JSON-line frames (`show`/`hide`/`quit`), draws with the resolved theme tokens and refuses to draw without an anchor or below 320 px.

**Measured:**

- **Focus (ADR-002 kill criterion), macOS + iTerm2, run by the user with `tools/focus-check.sh`:** the first run **failed**: the popup became the frontmost app for about a second at launch (winit activates the app at startup by default, even as an accessory app). After `with_activate_ignoring_other_apps(false)` the check **passed**: the frontmost app stayed iTerm2 for the whole 4 s, sampled every 0.2 s. This covers the frontmost app only, not whether every keystroke kept reaching the terminal.
- Rendered frames (screenshots read back from the GPU framebuffer): dark and light themes, windowed list with `11/12`, fuzzy highlights from per-item ranges, long label ellipsised with its description dropped, combining mark kept whole, CJK rendered once `system_fonts` is enabled (without it, boxes).
- `goh-popup` registers with macOS as type `UIElement`, an app class that cannot become the frontmost application.
- Everything else in the workspace still passes (tests, clippy).

**Not verified:**

- Keyboard events still reaching the terminal while the popup is shown (the script only checks the frontmost app); behaviour on Windows and Linux (X11/Wayland); whether `with_active(false)` / `with_mouse_passthrough` behave the same there.
- **Only the zsh adapter drives the overlay** (next sections); the bash adapter uses native completion and never will.
- Accessibility: the popup emits `ListBox`/`ListBoxOption` nodes with labels and a selected state through egui's AccessKit integration, but `active-descendant`, the polite status region and any VoiceOver/Narrator/Orca behaviour are untested (U12 at screen-reader level, SPIKE-06).
- Placement: flip-above and clamping to the caret's display are implemented and tested (see next section); a scale-1 display and a display layout with negative coordinates were not exercised.
- Match highlighting uses colour plus underline; bold is not drawn because egui ships no bold monospace face.

### Anchor provider and the verified-only gate (built)

**`crates/goh-anchor`** reads the caret of the focused terminal through the macOS Accessibility API (raw FFI, no Objective-C) and returns `Verified` only for apps that were measured (iTerm2, Terminal.app), identified by **executable path**, not by name. It also requires: `TERM_PROGRAM` matching the focused app (so the caret belongs to the shell's own terminal), no `tmux`/`screen`, no ssh, Accessibility trust, and a rectangle that passes sanity checks. Rules from S-04: iTerm2 uses a zero-length range and the tall-rectangle rule (its cell height comes from `OSC 1337 ; ReportCellSize` on the tty, about 100 ms at most); Terminal.app uses the one-character range and only the rectangle's top-left and height. Accessibility calls have a 250 ms messaging timeout. The display holding the caret comes from `CGDisplayBounds` (global, top-left origin, the same space as the caret).

**`goh-popup`** now accepts `"anchor": "auto"` and resolves the display mode itself with `goh_popup::display::resolve`, then places the window with `goh_popup::placement` (below the caret line by default, above when there is no room, clamped to the caret's display) and prints a one-line JSON decision. A `show` frame without a verified anchor draws nothing. `goh-popup --resolve` prints the decision for the focused terminal without opening a window.

**Verified here (no Accessibility access in my sandbox):**

- **Replay of the recorded probe data:** all 192 samples (4 runs × 48) map to their grid cells through the production rules (`cargo test -p goh-anchor`), including iTerm2's tall rectangles and Terminal.app's wide end-of-line rectangles.
- **Gate:** with no Accessibility access, `"anchor":"auto"` resolves to inline in all three modes (a forced `overlay` does not bypass it, and the reason says so); no anchor at all resolves to inline. `--resolve` prints the reason.
- **Environment checks** (TERM_PROGRAM mismatch, tmux, screen, ssh) and **placement** (flip above, right-edge clamp, second display with a different origin and size) have unit tests.
- **Where the window really lands**, read back from the window server (`CGWindowListCopyWindowInfo`) for a position vouched for by the caller: anchor (300, 300, line height 18) gives a window at (300, 330), 430 × 252, floating layer; anchor y=900 on a 982-pt-high screen flips above, at y=636; anchor x=2000 on the second display (x 1512, 1600 × 1000, reported by `CGDisplayBounds`) lands at x=2000. So winit's outer position is the same coordinate space as Accessibility rectangles and window frames, on both displays.

**First live run (user, iTerm2, `overlay-demo.sh`):** the popup process resolved `overlay`, "anchor verified and focus preserved", at x=68, y=659, `above: true`. That is consistent with the S-04 geometry to the point: iTerm2's cell is 9 × 19 pt with grid origin x=5, so the caret after the 7 typed characters of `$ git c` is at x = 5 + 7 × 9 = 68; the caret was on the bottom screen row (y = 923 in the iTerm2 probe), so there was no room below and the popup went above it: 923 − 12 − 252 = 659. This is the first time the Accessibility FFI, focused-element lookup and `ReportCellSize` read ran against a real terminal, and they produced the expected caret. The user's report contained only the printed output; whether the window was visibly at the caret was not confirmed.

**Second live run (user, iTerm2, clean `zsh -f` with `PS1='$ '`, demo script):** the gate call (cursor at column 1) reported a caret at x=445; the popup (cursor at column 8) at x=508 = 445 + 7 × 9, exact. In y, the gate call was on screen row 3 (y=261) and the popup's caret at y=394 = 261 + 7 × 19, i.e. row 10; the terminal said row 8 for the first `$ git c`, and the script printed a second `$ git c` two rows lower (a bug in the demo, since fixed), so the popup was anchored to that second one. **An earlier worry in this thread, that x=445/508 did not match `$ git c` at x≈68, was my mistake:** the grid origin depends on where the iTerm2 window is, and it had been moved (origin now about x 445, y 223 instead of 5, 68). With the origin read from the first call, every reported position matches its cell. The demo now asks the terminal for the cursor position (`ESC[6n`) without moving it and prints it after the popup, so a run gives one clean sample.

**Third live run (user, iTerm2, same clean shell, fixed demo): an out-of-sample match.** The terminal reported its cursor at **row 25, column 8** (`ESC[6n`, asked without moving it). The window origin (x0 = 445, y0 = 223) had been derived from the previous run, so the prediction for that cell was x = 445 + 7 × 9 = 508 and y = 223 + 24 × 19 = 679. The provider's caret was **x = 508, y = 679** (popup at y = 710 = 679 + 19 + 12, below the line). Exact, in both axes, on a cell not used for calibration. The user confirmed that the popup was visibly next to the `$ git c` line ("có"; an earlier garbled reply, "Cps", was the same answer). That is a yes/no confirmation, not a description of what was seen.

**Bug found by that run:** `goh-popup --resolve`, run in the same terminal just before, reported "no focused application". **First fix attempt failed:** I guessed a transient start-up failure and added a bounded retry (5 × 60 ms). The user's second run still failed, now with the code visible: **Accessibility error −25204 (`kAXErrorCannotComplete`)** from the system-wide "focused application" query, while the popup process, same terminal and permission, succeeded. So it is not transient. **Measured next (user, iTerm2, `goh-popup --resolve` with a trace):** `direct: error -25204`, `after CoreGraphics call: error -25204`, `after NSApplicationLoad: ok`, and the provider returned iTerm2's caret (x 445, y 926, height 19, display 1512 × 982) with `display: overlay`. So a plain command-line process cannot ask Accessibility for the focused application until it has connected to the window server **through AppKit**: a CoreGraphics call alone is not enough. (A GUI process such as the popup is already connected, which is why it worked all along.) `locate()` now tries the plain call and, on failure, calls `NSApplicationLoad()` and retries; the CoreGraphics step was dropped. This also means a CLI user of the provider (a future `goh-ctl anchor`) pays one `NSApplicationLoad` and may show a Dock icon briefly; not yet checked.

**Not verified:** Terminal.app (not yet run through `overlay-demo.sh` or `--resolve`); (popup visibly at the caret in iTerm2: confirmed by the user, yes/no only); whether `NSApplicationLoad` shows a Dock icon for `--resolve`; a live zsh/bash prompt other than the demo line; scale-1 displays; iTerm2 split panes (the provider cannot tell a split pane from a single one, so it can report "verified" there); the retry fix.

### zsh adapter drives the popup (built)

`adapters/zsh/goh.zsh` now sends the menu to `goh-popup` (details in `adapters/zsh/README.md`): lazy background start, frames over FIFOs, the answer read asynchronously with `zle -F`, the inline menu shown at once and hidden only when the popup answers "overlay", `hide` on every close (accept, Esc, edit, Enter), `quit` when the shell exits. `goh-ctl menu` now also returns each item's match ranges; the bash adapter ignores the extra field. **Terminal.app and iTerm2 are handled alike**, because the verified-only gate lives in the popup/provider, not in the adapter.

One design problem found before wiring: the provider asked iTerm2 for its cell height on the tty. Inside the line editor, that reply is terminal input and would land in the command line. The adapter therefore asks once at load (`goh-ctl theme --probe` prints `cell-height`), passes it in each frame, and the popup refuses to query the tty unless a frame says `"tty_query": true` (only the demo does).

**Verified here:** 5 new pty tests with a stand-in popup (it logs frames and answers `overlay`/`inline`): frames carry the items, match ranges, selection, context and theme; Down changes `selected`; Esc, accept and edits send `hide`; an `overlay` answer makes the adapter stop drawing the inline list and an `inline` answer keeps it; `GOH_DISPLAY=inline` never starts the popup; a popup that dies at once leaves the inline menu working and the shell responsive even after 400 further frames; leaving the shell sends `quit`. Mutation checks: removing the `hide` call, ignoring the overlay answer, and dropping the liveness check each make a test fail. A frame built by the adapter (quotes and backslashes in labels included) is accepted by the real `goh-popup` binary.

**Bug found while testing:** a test that blindly writes into a dead popup's FIFO only fails once the pipe buffer is full; my first version wrote too few frames and could not tell a missing liveness check from a present one. It now sends 400 frames. (A test-harness write also needed to cope with a full pty.)

**Live report (user, iTerm2): "no popup appears when typing `git c`", and what it turned out to be.** `goh-status` showed the adapter, `goh-ctl`, the spec and the gate all fine, but `query 'git c' -> 0 candidate(s)` and `popup ... running: no`. Cause, in part my own doing: **my test commands used `pkill -x gohd` and `pkill -x goh-popup`, and the sandbox shares the user's account, so they also killed the user's real daemon and popup.** A dead daemon leaves its socket file behind (so `goh-status` still said "running", because it only checked that the file exists), and the first request after that was swallowed while `goh-ctl` merely relaunched the daemon. Tests now only stop the daemon that holds their own temporary socket. Fixes: `goh-ctl` now starts a missing daemon, waits up to 0.5 s for its socket and asks once more (the first Tab after a cold start or a crash is no longer lost; if the daemon cannot start, the request fails within that bound); tests cover the cold start, a stale socket after the daemon is killed, and a daemon that cannot start. Re-sourcing the adapter now stops a popup left by the previous load, and `goh-status` shows the popup's process state, the theme and cell height, the last decision and the last frame. The same run showed `cell-height 19` was present, so the "iTerm2 cell height missing" suspicion was wrong. **What the overlay itself does in the user's iTerm2 session is still not known**: the report was taken after the processes had been killed, and no menu had been opened in that shell yet.

**Live confirmation (user, iTerm2, two screenshots; adapter + real `goh-popup`, prompt is a two-line starship prompt with emoji):** after `git c` + Tab, the overlay appeared as a floating window just below the caret line, its left edge at the caret, listing `checkout`, `cherry-pick`, `commit` with their descriptions, a `❯` on the selected row, and a footer with `git` and the key hints; the inline list was not drawn, and the ghost text `heckout` was shown in the command line. After ↓ the selection moved to `cherry-pick` and the ghost text changed to `herry-pick`. The popup used the terminal's own colours (theme source "verified"). The window extended below the terminal window, as a separate floating window does. This shows end to end, on a real terminal, that: the adapter starts the popup, frames reach it, the Accessibility lookup gives a usable caret in a real shell prompt (the starship prompt did not break it, contrary to an earlier worry), the popup answers "overlay" and the adapter hides its inline list, and ↓ reaches the shell while the popup is visible (so it did not take the keystroke).

**Bug in those same two screenshots, found on a second look (the user asked whether it was fixed; I had read the pictures as a success and missed it):** measured from the images, the popup was 22.0 pt below the centre of the command line after the first Tab (the placement rule gives 21.5 pt) but **39.5 pt after pressing ↓, about one row too low.** Hypothesis, not reproduced (no Accessibility access in my sandbox): the popup reads the terminal cursor through Accessibility just as zsh redraws the line, when the cursor is briefly one row lower (zsh moves it down to clear the message area and back). Two changes: the adapter now finishes its redraw (`zle -R`) before sending the frame, and the provider re-reads the caret every 25 ms (up to 4 more times) until two reads agree, so a transient position is not trusted (`--resolve` traces "caret: stable" or "still moving"). The settle helper has unit tests; **after rebuilding and re-testing in iTerm2 the user reported the drift gone ("hết rồi", a short reply; no measurements or screenshots this time).** The cause was never reproduced here, so the redraw/settle explanation remains a hypothesis that the result is consistent with; the fix would not help if the cursor were really one row lower for longer than ~100 ms.

### As-you-type mode (requested by the user)

**Requested behaviour:** type a command and the popup appears and follows what is typed, only when the command is valid; previously the menu opened on Tab and any further keystroke closed it. **Built:** `GOH_LIVE=on` (default; `off` = Tab only) in the zsh adapter, a `live` flag on the IPC `complete` request (`goh-ctl menu --live`), and the engine's `Completion { candidates, valid }`. The definition of "valid" and the alternatives considered are in docs/docs/06-completion-engine.md. The adapter refreshes from the `zle-line-pre-redraw` hook only after typing or deleting widgets (not history recall or autosuggest accepts), skips the refresh while input is queued (`$PENDING`), keeps ↑/↓/Esc/Tab bound only while the menu is open, and does not reopen the menu by itself after an accept. One query measured 3.4 ms median, 4.7 ms p95 (40 runs each, against a running daemon), so a query per pause in typing is cheap.

**Verified here:** engine validity rules (6 tests), the IPC flag (an explicit request is unaffected, a live one gets nothing for an invalid line), and pty tests of the adapter with the stand-in popup and the inline menu: typing alone opens it, each keystroke narrows candidates and ghost text, a no-match closes it and deleting reopens it, `zzz c`, `git foo c`, `gi` do not open it while `git ` does, Tab still completes an unknown command's files, Esc closes it until the next keystroke, accept does not reopen it, history recall does not open it, updates and a `hide` reach the popup, and `GOH_LIVE=off` restores Tab-only. Mutation checks: treating every edit as typing and dropping `--live` each made a test fail. A first version had an "Esc dismissal" flag that no test could tell from its absence (the hook never refreshes without a change anyway); it was removed.

**Not verified:** this mode with the real popup in a live iTerm2 (flicker, lag per keystroke, whether the popup follows the caret smoothly as the line grows, whether it feels intrusive for commands with many arguments); behaviour with prompt plugins that redraw on every key (zsh-autosuggestions and syntax highlighting also use this hook); very fast typing on a slow machine; wrapped long lines; and the bash adapter, which has no as-you-type mode.

### Seeing the friction without a screen: the typing simulator (`tools/typing-sim`)

The user asked how the assistant could notice inconveniences itself. It cannot see a real screen or use Accessibility, so a simulator replays realistic typing (13 command lines: git, docker, npm, unknown commands, typos) in a real zsh with the real daemon and the adapter's new `GOH_TRACE` log, and prints per keystroke whether the menu is open and with how many candidates, the adapter's cost, the frames sent, and a list of friction. It also renders the real popup for frames the adapter actually sent (`--save-frames`). Realistic specs for git (20 subcommands), docker (12) and npm (10) were generated from the prototype's data (`docs/examples/specs/`) so the simulation is not limited to the 4-command test spec.

**Found and fixed with it:**

1. **Menu lingering after a finished word** (`git status` typed in full still showed a one-row `status`). My first fix, closing it, made it **blink at every word boundary** (closed on the last letter, open again on the space); the simulator showed that at once (`FLICKER`). Final behaviour: a finished word does not *open* a menu, but an open one is kept so it moves straight to the next list. The remaining one-row popup (visible in a rendered frame: `main  argument`) is the accepted cost.
2. **Scattered (fuzzy) matches as noise**, seen in the rendered popup: typing `c` listed `branch` and `fetch`; typing `checko` listed `check-ignore`. Now fuzzy matching needs two typed characters, and unasked suggestions drop fuzzy matches when any prefix match exists.
3. **Options matched against a plain word**: typing the remote `origin` after `git push` offered `--force-with-lease` (`o…r` in order). Options are now matched only for an empty word or one starting with `-`.
4. **Popup frame backlog**: 105 frames for 228 keystrokes, each costing an Accessibility read in the real popup; the popup now applies only the newest queued frame.
5. Not changed, a taste setting: the menu opens on a bare space after a command (`git ` lists 8 of 20 subcommands); `GOH_LIVE_MIN=1` waits for the first typed character.

**Numbers (13 command lines, 228 keystrokes):** adapter time per key median 12.8 ms, p95 18.1 ms, max 28.8 ms at 90 ms/key; median 8.1 ms at 25 ms/key (queued keys are refreshed once). After the fixes the simulator reports no flicker and one design-level item.

**Limits of the simulator:** it sees the adapter's decisions and costs with a stand-in popup, not the real window, so nothing about on-screen blinking, real Accessibility cost or how it *feels*; its corpus and friction rules are only what was thought of. It is a way to find regressions and obvious noise without a screen, not a replacement for the user's eyes.

**Bug: the overlay stayed on top of other apps after iTerm2 was minimised (user screenshot: a popup listing `bash/` and `zsh/`, footer `docker`, floating over another application).** Cause: the popup was hidden only by a `hide` frame, which the adapter sends when the menu closes, i.e. on a keystroke; minimising or hiding the terminal (or switching to another app) with the menu open sends nothing, so the always-on-top window was left behind. Fix: while a menu is open the popup now re-checks the terminal every 300 ms (`goh_popup::watch::plan_tick`): it **hides** when the caret can no longer be verified (another app focused, terminal minimised or hidden, window closed), **follows** the terminal if its window moves, and **comes back** if the terminal returns while the menu is still open; a `hide` frame ends the checking. The checks are silent: only `show` frames produce a decision line for the adapter. Tested: `plan_tick` has unit tests, and the real popup was driven through minimise, return, move and hide with a test anchor read from a file (`"anchor":"file:PATH"`, a caller-vouched position re-read on each check, empty file = terminal not usable); the window server confirmed hidden, shown at the right place, moved by the same offset, hidden again, and not re-shown without a menu. **Not verified with the real Accessibility path in a live iTerm2** (the sandbox has none): that minimising really makes the provider fail as assumed (another app should become the focused application, so the exe-path check fails), and the cost of one Accessibility read every 300 ms while a menu is open. Known limit: switching to another **tab of the same iTerm2 window** while the menu is open is not detected (the focused element is still iTerm2's), so the popup would follow the other tab's caret.

**Not verified:** how long the first overlay takes after the popup starts (the screenshots do not say); Esc, accept and Enter with the real popup (only the stand-in popup tests cover them); flicker or lag while moving; Terminal.app live (every real run so far was iTerm2); a scale-1 display; iTerm2 split panes; very long lines that wrap; and the overlay's cost per ↑/↓ (one Accessibility lookup each). The automated tests use the stand-in popup; the real chain is checked by hand.

**Decision:** keep egui/winit as the renderer. The overlay sits behind the verified-only gate and the zsh adapter drives it (default `GOH_DISPLAY=auto`; `inline` switches it off). iTerm2 passed the live demo and a live end-to-end session. **Terminal.app is enabled on the strength of the probe data and the replayed rules only; the user decided not to test it live, so its Accessibility lookup, focus behaviour and popup placement in a real session are unverified. If it misbehaves, `GOH_DISPLAY=inline` turns the overlay off, or Terminal.app can be removed from `App::from_exe_path` in `crates/goh-anchor/src/rules.rs`.** Focus must be re-checked on any new platform.

## Next runs, in order of value

1. Use it live: in iTerm2, `source adapters/zsh/goh.zsh` with `goh-popup` on `PATH`, type `git c` + Tab, move with ↑/↓, accept, Esc; note how long the first overlay takes and whether anything flickers or steals a key.
2. (Skipped by the user) The same in Terminal.app (enable it under System Settings > Privacy & Security > Device Control and Data Access, then reopen it): `goh-popup --resolve` should say `"app":"Terminal"`.
3. Starship-style two-line prompts with emoji, a scale-1 display, iTerm2 split panes.
4. Bash on Ubuntu with GNOME Terminal and Kitty (real emulators), and bash 4.x if `COMP_POINT` handling is to be relaxed.
5. PowerShell 7 on Windows 11: S-03 plus the named-pipe transport.
