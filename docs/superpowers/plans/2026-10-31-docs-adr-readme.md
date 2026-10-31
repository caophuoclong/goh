# Decision Records and README Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the decision records match what goh actually does, and make the README show at a glance what goh looks like, where its menu appears in each terminal, and what sets it apart.

**Architecture:** Docs only. `docs/docs/12-adr.md` already holds ADR-001 to ADR-010, all marked `Proposed` from before the first release; several are now settled in code and one (ADR-008) says the opposite of what shipped. The README's `## Status` still says "Version 1.0" (the release is 2.1.0) and has no picture. A demo GIF is recorded with VHS from a script checked into the repo, so it can be re-recorded after any UI change.

**Tech Stack:** Markdown, [VHS](https://github.com/charmbracelet/vhs) (`brew install vhs`) for the recording.

**Spec:** `docs/docs/12-adr.md`, `README.md`, `docs/features/README.md` (feature index).

## Global Constraints

- Conventional Commits with the `docs:` type (no release); end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Runs after `2026-10-31-inline-box-menu.md` (the README table and the recording show the box) and after `2026-10-31-more-bundled-specs.md` (the README states the spec count). The table rows for bash with ble.sh, PowerShell and Nushell come from `2026-10-31-bash-blesh-menu.md` and `2026-10-31-powershell-nushell-menus.md`: drop a row whose plan has not shipped and keep that shell in the last row.
- Every claim in the README must be true of the code at the commit it lands in; check each one against `docs/features/` before writing it.
- The README does not name or compare other products.
- The GIF stays under 3 MB.

---

## File Structure

| File | Change |
|---|---|
| `docs/docs/12-adr.md` | Statuses and ADR-008 brought up to date |
| `docs/media/demo.tape` | VHS script for the demo |
| `docs/media/demo.gif` | The recording |
| `README.md` | Demo at the top, "Where the menu appears" table, `## Status` fixed |

---

### Task 1: Bring the decision records up to date

**Files:**
- Modify: `docs/docs/12-adr.md`

- [ ] **Step 1: Check each record against the code**

Run each check; a record is `Accepted` only if its check holds.

| ADR | Check | Holds when |
|---|---|---|
| 001 Rust core | `ls crates/*/Cargo.toml` | all components are Rust crates |
| 002 egui + winit | `grep -n 'eframe\|winit' crates/goh-popup/Cargo.toml` | both present |
| 003 Capability-based overlay | `grep -n '"auto" \| "overlay" \| "inline"' crates/goh-daemon/src/config.rs` | the three modes exist |
| 004 Own static schema | `ls docs/schemas/completion.schema.json && grep -rn 'quickjs\|rquickjs\|boa' Cargo.lock` | schema exists, no JS engine |
| 006 Shell editor, no key tap | `grep -rln 'CGEventTap\|keylogger' crates` | nothing found |
| 007 Theme per session | `ls docs/design-system/theme-resolution.md` | exists |
| 009 Popup is presentational | `grep -n 'never takes focus\|ADR-009' docs/features/popup.md crates/goh-popup/src/main.rs` | the popup never takes keys |
| 010 tokens.json single source | `ls docs/design-system/tools` and read what is there | a generator for `tokens.css` exists |

- [ ] **Step 2: Update statuses**

For each record whose check holds, change `**Status:** Proposed` to `**Status:** Accepted (2.1.0)`. Leave the others `Proposed` and add one line under them: `- **Open:** <what the check found missing>.`

- [ ] **Step 3: Rewrite ADR-005 and ADR-008 to match what shipped**

Replace the ADR-005 block with:

```markdown
## ADR-005 — Command text stays in memory unless the user turns recording on

- **Status:** Accepted (2.1.0), amended · **Decision:** command buffers are ephemeral and never logged. Keeping the
  lines a user runs is opt-in (`privacy.recordCommands`, off by default) and goes to one private file (mode 0600)
  that `goh forget` deletes; lines that look like secrets or start with a space are never written. No telemetry.
- **Why amended:** finishing a line from history, next-command prediction and remembered option values need the
  record; making it opt-in keeps the original promise for everyone who does not ask for them.
- **Alternative rejected:** always-on usage analytics.
```

Replace the ADR-008 block with:

```markdown
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
```

- [ ] **Step 4: Commit**

```bash
git add docs/docs/12-adr.md
git commit -m "docs: bring the decision records up to date"
```

---

### Task 2: Record the demo

**Files:**
- Create: `docs/media/demo.tape`, `docs/media/demo.gif`

- [ ] **Step 1: Install VHS and build goh**

```bash
brew install vhs
cargo build --release -p goh-daemon
```

- [ ] **Step 2: Write the tape**

`docs/media/demo.tape`:

```text
# Re-record after a UI change: vhs docs/media/demo.tape (from the repository root, after a release build).
Output docs/media/demo.gif
Set Shell zsh
Set FontSize 18
Set Width 1100
Set Height 560
Set Padding 16
Set TypingSpeed 90ms
Set Theme "Catppuccin Mocha"

Hide
Type "export GOH_CTL=$PWD/target/release/goh GOH_DISPLAY=inline GOH_LIVE=on PS1='%F{blue}~/project%f ❯ '"
Enter
Type "source adapters/zsh/goh.zsh && clear"
Enter
Show

Sleep 600ms
Type "git chec"
Sleep 1s
Tab
Sleep 400ms
Type "-"
Sleep 1s
Ctrl+U
Sleep 400ms

Type "docker run --"
Sleep 1s
Down
Down
Sleep 800ms
Escape
Ctrl+U
Sleep 400ms

Type "git push --fo"
Sleep 1500ms
Escape
Ctrl+U
Sleep 400ms

Type "git chekcout"
Sleep 1500ms
Escape
Ctrl+U
Sleep 600ms
```

It shows, in order: subcommand completion and Tab, option completion with descriptions, moving in the menu, the risk warning on `--force`, and the typo correction. Run it from a git repository (this one) so `git` has branches to show.

- [ ] **Step 3: Record and check the size**

```bash
vhs docs/media/demo.tape
ls -l docs/media/demo.gif
```

Expected: the GIF exists and is under 3 MB. If larger, lower `Set Width` to 960 and `Set Height` to 480 and record again. Open it and check that each scene shows the framed menu.

- [ ] **Step 4: Commit**

```bash
git add docs/media/demo.tape docs/media/demo.gif
git commit -m "docs: add a recorded demo"
```

---

### Task 3: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Demo under the first paragraph**

After the paragraph ending `It reproduces what Fig was useful for, not its branding, and has no dependency on Amazon Q CLI.`, add:

```markdown
![goh completing git and docker commands in zsh](docs/media/demo.gif)
```

- [ ] **Step 2: Where the menu appears**

Add a section after `## What it does`:

```markdown
## Where the menu appears

| Where | How the menu is drawn | Needs |
|---|---|---|
| iTerm2, Terminal.app, VS Code, the Claude app (macOS) | a popup at the cursor, in the terminal's font and colours | the Accessibility permission (`goh doctor --fix`) |
| Linux X11 / XWayland terminals that expose AT-SPI | a popup at the cursor | `libxkbcommon-x11`, EGL/OpenGL |
| Ghostty, Kitty, WezTerm, Alacritty, Zed, Rio, any other terminal | a floating framed box under the word, drawn by the shell | nothing |
| tmux, screen, native Wayland | the floating framed box | nothing |
| ssh with the zsh adapter | the popup on the machine you connected from (`goh ssh`), else the floating box | goh on that machine |
| bash with ble.sh | ble.sh's menu with descriptions, opening as you type | bash 4+ and ble.sh |
| PowerShell 7.2+ | PSReadLine's prediction list, as you type | PSReadLine 2.2.2+ |
| Nushell | a bordered menu that opens at each new word | nothing |
| bash, fish, elvish, xonsh | the shell's own completion list on Tab, filled by goh | nothing |

`display.inlineStyle: "flat"` replaces the box with plain rows, which screen readers read more easily.
```

Before committing, check each row against `docs/features/popup.md`, `docs/features/zsh-adapter.md`, `docs/features/ssh-popup.md` and `docs/features/other-shells.md`, and drop or fix any row they do not support.

- [ ] **Step 3: Fix `## Status`**

Replace the section's first paragraph with:

```markdown
Version 2.1 (see [CHANGELOG.md](CHANGELOG.md)). zsh on macOS and Linux is the main target and is tested on a real
pseudo-terminal in CI; bash and fish are tested in CI too. PowerShell, Nushell, elvish and xonsh adapters are
experimental. Each feature's state is in [docs/features](docs/features/README.md).
```

Use the version in `.release-please-manifest.json` at the time of the commit.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: show the demo and where the menu appears"
```
