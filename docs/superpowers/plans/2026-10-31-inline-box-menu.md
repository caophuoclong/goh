# Inline Box Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In every terminal without the native popup (Ghostty, Kitty, WezTerm, Alacritty, Zed, Rio, tmux, ssh, iTerm2 without the Accessibility permission), draw the zsh inline menu as a floating framed box anchored under the word being completed, so it reads as a popup without any OS permission.

**Architecture:** The inline menu is already drawn by ZLE through `POSTDISPLAY` + `region_highlight` (`adapters/zsh/goh.zsh`, `_goh_render`). ZLE owns those lines, so redraw, scroll and clearing stay correct in any terminal, tmux and ssh included. This plan keeps that renderer and wraps its rows in a border, indents them to the screen column of the word, and fills them with the theme's surface colour. No PTY proxy, no raw escape codes written behind ZLE's back. A new setting `display.inlineStyle` (`box`, default, or `flat`, today's look) keeps the flat list for screen-reader users.

**Tech Stack:** zsh 5.8+ (styles need 5.9's `memo=`), Rust tests driving a real zsh on a pseudo-terminal (`crates/goh-daemon/tests/zsh_pty.rs`).

**Spec:** `docs/features/zsh-adapter.md`, `docs/design-system/README.md` (section "Inline menu"), `docs/docs/12-adr.md`.

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Commit messages follow Conventional Commits (`feat(zsh): ...`, `test(zsh): ...`, `docs: ...`). End every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Shapes, not colour, carry every cue: under `NO_COLOR` the border, `❯` marker and bold stay; only colours go.
- Fail-open: if the prompt cannot be measured, the box is only shifted left; it never breaks the line.
- `GOH_*` environment variables are public surface (CONTRIBUTING.md): `GOH_INLINE_STYLE` is new, accepted values `box` and `flat`.
- Before pushing: `cargo test --workspace --locked`, `shellcheck install.sh scripts/*.sh`, `scripts/check-feature-specs.sh`.

---

## File Structure

| File | Change |
|---|---|
| `adapters/zsh/goh.zsh` | New `_GOH_INLINE_STYLE` variable, new `_goh_word_column`, box wrapping at the end of the row layout in `_goh_render` |
| `crates/goh-daemon/src/config.rs` | Validate `display.inlineStyle`, export it as `GOH_INLINE_STYLE` |
| `docs/examples/config.json`, `docs/examples/config.jsonc` | The new setting with its default and comment |
| `docs/schemas/config.schema.json` | `display.inlineStyle` enum |
| `crates/goh-daemon/tests/common/mod.rs` | Existing PTY tests pin `GOH_INLINE_STYLE=flat` (their expectations are the flat rows) |
| `crates/goh-daemon/tests/zsh_pty.rs` | New tests for the box |
| `docs/features/zsh-adapter.md`, `docs/design-system/README.md`, `docs/docs/12-adr.md` | Behaviour, design rule, decision record |

---

### Task 1: `display.inlineStyle` setting

**Files:**
- Modify: `crates/goh-daemon/src/config.rs:143-190` (`validate`, `env_lines`) and its tests at `:221-290`
- Modify: `docs/examples/config.json`, `docs/examples/config.jsonc`, `docs/schemas/config.schema.json`

**Interfaces:**
- Produces: `goh config env` prints `GOH_INLINE_STYLE=box` or `GOH_INLINE_STYLE=flat` when the file sets `display.inlineStyle`. The zsh adapter (Task 2) reads `GOH_INLINE_STYLE`.

- [ ] **Step 1: Write the failing tests**

In `crates/goh-daemon/src/config.rs`, change the expected list in `the_template_is_valid_and_exports_the_adapter_settings` and add a validation case:

```rust
        assert_eq!(env_lines(&v), ["GOH_DISPLAY=auto", "GOH_INLINE_STYLE=box", "GOH_LIMIT=8", "GOH_LIVE=on", "GOH_PREDICT=off", "GOH_ENTER_RUNS=on", "GOH_RECORD=off", "GOH_UPDATE_CHECK=off", "GOH_POPUP_OWN_PERMISSION=off", "GOH_POPUP_SSH=on"]);
```

```rust
    #[test]
    fn the_inline_style_is_box_or_flat() {
        assert!(validate(r#"{"display":{"inlineStyle":"flat"}}"#).is_ok());
        assert!(validate(r#"{"display":{"inlineStyle":"round"}}"#).unwrap_err().contains("display.inlineStyle"));
        let v: Value = serde_json::from_str(r#"{"display":{"inlineStyle":"round"}}"#).unwrap();
        assert!(env_lines(&v).iter().all(|l| !l.starts_with("GOH_INLINE_STYLE")), "a bad value is not exported");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p goh-daemon --lib config::tests`
Expected: FAIL. The template test lacks `GOH_INLINE_STYLE=box`, and `validate` accepts `"round"`.

- [ ] **Step 3: Implement**

In `validate`, after the `display.mode` check:

```rust
    if let Some(style) = v.pointer("/display/inlineStyle") {
        if !matches!(style.as_str(), Some("box" | "flat")) {
            return Err(format!("display.inlineStyle must be \"box\" or \"flat\", not {style}"));
        }
    }
```

In `env_lines`, right after the `GOH_DISPLAY` push:

```rust
    if let Some(s) = v.pointer("/display/inlineStyle").and_then(Value::as_str).filter(|s| matches!(*s, "box" | "flat")) {
        out.push(format!("GOH_INLINE_STYLE={s}"));
    }
```

In `docs/examples/config.json`, inside `"display"` after `"mode": "auto",` add `"inlineStyle": "box",`.

In `docs/examples/config.jsonc`, after the `"mode": "auto",` line:

```jsonc
    // How the menu looks when it is drawn in the terminal (not in the popup).
    //   "box":  a framed box under the word you are completing, like a popup
    //   "flat": plain rows under the line, with no frame (easier for a screen reader)
    "inlineStyle": "box",
```

In `docs/schemas/config.schema.json`, under `display.properties`, after `"mode"`:

```json
        "inlineStyle": {
          "enum": ["box", "flat"],
          "default": "box",
          "description": "How the menu looks when drawn in the terminal: a framed box under the word (box) or plain rows (flat)."
        },
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p goh-daemon --lib config::tests`
Expected: PASS, including `the_template_is_rebuilt_exactly_from_its_own_settings` (the template has one setting per line).

- [ ] **Step 5: Commit**

```bash
git add crates/goh-daemon/src/config.rs docs/examples/config.json docs/examples/config.jsonc docs/schemas/config.schema.json
git commit -m "feat(config): add display.inlineStyle (box or flat)"
```

---

### Task 2: Draw the inline menu as an anchored box

**Files:**
- Modify: `adapters/zsh/goh.zsh` (variables near `:57`, `_goh_render` at `:407-559`)
- Modify: `crates/goh-daemon/tests/common/mod.rs:165`
- Test: `crates/goh-daemon/tests/zsh_pty.rs`

**Interfaces:**
- Consumes: `GOH_INLINE_STYLE` from Task 1 (`box` | `flat`, default `box`).
- Produces: `_goh_word_column` sets `REPLY` to the 0-based screen column where the word being completed starts. `_goh_render` emits, in box style, `POSTDISPLAY` lines `<indent>╭─…─╮`, `<indent>│<row padded to inner>│` …, `<indent>╰─…─╯`.

- [ ] **Step 1: Pin the existing tests to the flat style**

In `crates/goh-daemon/tests/common/mod.rs`, after `.env("GOH_LIVE", "off")`:

```rust
            .env("GOH_INLINE_STYLE", "flat") // existing tests read the flat rows; box tests switch it on
```

Run: `cargo test -p goh-daemon --test zsh_pty`
Expected: PASS (nothing reads the variable yet).

- [ ] **Step 2: Write the failing tests**

Append to `crates/goh-daemon/tests/zsh_pty.rs`:

```rust
const BOX: &[(&str, &str)] = &[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "box")];

#[test]
fn the_box_frames_the_rows_under_the_word_being_completed() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(BOX, true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, h) = menu(&fx, &mut sh);
    // rows[0] is the ghost text on the line itself; the box follows.
    let top = &rows[1];
    let indent = top.chars().take_while(|c| *c == ' ').count();
    let inner = top.trim_start().chars().count() - 2;
    assert!(top.trim_start().starts_with('╭') && top.ends_with('╮'), "{rows:?}");
    // `READY> ` is 7 columns and `git ` 4 more: the word starts at column 11, unless the box must move left to fit 60.
    assert_eq!(indent, 11.min(59 - inner - 2), "{rows:?}");
    let body: Vec<&String> = rows[2..].iter().take_while(|r| !r.trim_start().starts_with('╰')).collect();
    assert!(body.iter().all(|r| r.chars().count() == indent + inner + 2 && r[..indent].trim().is_empty()), "every row has the box's width: {rows:?}");
    assert_eq!(body[0].trim_start(), format!("│{:inner$}│", "❯ ↳ checkout"));
    assert!(rows.iter().any(|r| r.trim_start().starts_with('╰')), "closed at the bottom: {rows:?}");
    assert!(h.contains("bold"), "the selected row stays bold: {h}");
}

#[test]
fn the_box_keeps_its_shape_without_colour() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "box"), ("NO_COLOR", "1")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, h) = menu(&fx, &mut sh);
    assert!(rows[1].trim_start().starts_with('╭') && rows.iter().any(|r| r.contains("│❯ ↳ checkout")), "{rows:?}");
    assert!(!h.contains("bg=") && !h.contains("fg="), "no colours under NO_COLOR: {h}");
}

#[test]
fn flat_style_keeps_the_rows_flush_left() {
    let fx = Fixture::new(require_env!(), true);
    let mut sh = fx.shell(&[("GOH_LIVE", "on"), ("GOH_INLINE_STYLE", "flat")], true);
    menu_dump(&fx, &mut sh);
    sh.send("git c", 400);
    let (rows, _) = menu(&fx, &mut sh);
    assert_eq!(rows[1], SELECTED, "{rows:?}");
    assert!(!rows.iter().any(|r| r.contains('╭')), "{rows:?}");
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p goh-daemon --test zsh_pty the_box_ flat_style`
Expected: the two `the_box_` tests FAIL because `rows[1]` is `❯ ↳ checkout` with no frame. `flat_style_keeps_the_rows_flush_left` PASSES.

- [ ] **Step 4: Add the style variable and the column helper**

In `adapters/zsh/goh.zsh`, after `typeset -g _GOH_DISPLAY=...` (line 57):

```zsh
typeset -g _GOH_INLINE_STYLE=${GOH_INLINE_STYLE:-box}   # box | flat (how the menu looks when drawn in the terminal)
```

After `_goh_byte_prefix` (line 387), add:

```zsh
# The screen column (from 0) where the word being completed starts: the width of the prompt's last line plus the
# line typed before the word. A prompt this cannot measure only moves the box; nothing else depends on it.
_goh_word_column() {
  emulate -L zsh
  setopt extended_glob
  local p=${(%)PS1} before
  p=${p//$'\e'\[[0-9;?]#[@-~]/}            # colours and other CSI sequences take no room
  p=${p//$'\e'\][^$'\a']#$'\a'/}           # nor do OSC sequences (titles, marks)
  p=${p##*$'\n'}
  _goh_byte_prefix "$BUFFER" ${_goh_start[_goh_sel]:-0}; before=$REPLY
  if [[ $before == *$'\n'* ]]; then p= before=${before##*$'\n'}; fi
  REPLY=$(( (${(m)#p} + ${(m)#before}) % ${COLUMNS:-80} ))
}
```

- [ ] **Step 5: Wrap the rows in the box**

In `_goh_render`, replace the first two lines of the body:

```zsh
  local i k n p s e row label shown risk desc room pad
  local -a rows hl
  local width=$(( ${COLUMNS:-80} - 1 ))
```

with:

```zsh
  local i k n p s e row label shown risk desc room pad
  local -a rows hl
  local width=$(( ${COLUMNS:-80} - 1 )) box=0
  # The box needs room for its two sides and a menu worth reading.
  if [[ $_GOH_INLINE_STYLE == box ]] && (( width >= 24 )); then box=1; width=$(( width - 2 > 60 ? 60 : width - 2 )); fi
```

Then, right after the notice block ends (after the line `(( _goh_overlay == 0 )) && _goh_notice_shown=1` and its closing `fi`, before `# Ghost text only where it is honest`), insert:

```zsh
  # Box style: the same rows inside a frame, filled with the surface colour and moved under the word being
  # completed, as far right as the terminal allows. Highlight offsets move with the rows.
  if (( box )); then
    local inner=1 r indent surface= selected= border=
    for r in "${rows[@]}"; do (( ${#r} > inner )) && inner=${#r}; done
    _goh_word_column; indent=$REPLY
    (( indent + inner + 2 > ${COLUMNS:-80} - 1 )) && indent=$(( ${COLUMNS:-80} - 1 - inner - 2 ))
    (( indent < 0 )) && indent=0
    if (( color )); then
      [[ ${_goh_theme[surface-base]-} =~ '^#[0-9a-f]{6}$' ]] && surface="bg=${_goh_theme[surface-base]}"
      [[ ${_goh_theme[surface-selected]-} =~ '^#[0-9a-f]{6}$' ]] && selected="bg=${_goh_theme[surface-selected]}"
      border=${muted:-fg=8}
    fi
    local edge= lead=
    edge=${(l:inner::─:)edge} lead=${(l:indent:: :)lead}
    local -a framed=("$lead╭$edge╮") moved=() f
    for r in "${rows[@]}"; do framed+=("$lead│${(r:inner:)r}│"); done
    framed+=("$lead╰$edge╯")
    local last=$#framed sel_row=$(( _goh_sel - _goh_win + 2 ))
    # Fills first, so the row styles that follow combine with them.
    if [[ -n $surface ]]; then
      for (( k = 1; k <= last; k++ )); do moved+=("$k $indent $(( indent + inner + 2 )) $surface"); done
    fi
    [[ -n $selected ]] && moved+=("$sel_row $(( indent + 1 )) $(( indent + 1 + inner )) $selected")
    if [[ -n $border ]]; then
      moved+=("1 $indent $(( indent + inner + 2 )) $border" "$last $indent $(( indent + inner + 2 )) $border")
      for (( k = 2; k < last; k++ )); do moved+=("$k $indent $(( indent + 1 )) $border" "$k $(( indent + inner + 1 )) $(( indent + inner + 2 )) $border"); done
    fi
    for h in "${hl[@]}"; do
      f=(${=h})
      moved+=("$(( f[1] + 1 )) $(( f[2] + indent + 1 )) $(( f[3] + indent + 1 ))${f[4]:+ $f[4]}")
    done
    rows=("${framed[@]}") hl=("${moved[@]}")
  fi
```

`h` is already declared further down (`local from=... h`); declare it here instead by adding `h` to the first `local` line of `_goh_render`:

```zsh
  local i k n p s e h row label shown risk desc room pad
```

and change the later declaration `local from=$(( ${#PREDISPLAY} + ${#BUFFER} )) h` to `local from=$(( ${#PREDISPLAY} + ${#BUFFER} ))`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p goh-daemon --test zsh_pty`
Expected: PASS, the three new tests and every existing one (they run with `GOH_INLINE_STYLE=flat`).

- [ ] **Step 7: Try it by hand in a terminal without the popup**

Run in Ghostty, Kitty or WezTerm (or any terminal with `GOH_DISPLAY=inline`):

```bash
cargo build -p goh-daemon && GOH_CTL=$PWD/target/debug/goh GOH_DISPLAY=inline zsh
```

then `source adapters/zsh/goh.zsh` in that shell and type `git c`, `docker r`, `cd ` at the left edge and near the right edge of a narrow window. Expected: a framed box starts under the word, slides left near the right edge, and disappears cleanly on Esc and on Enter.

- [ ] **Step 8: Commit**

```bash
git add adapters/zsh/goh.zsh crates/goh-daemon/tests/common/mod.rs crates/goh-daemon/tests/zsh_pty.rs
git commit -m "feat(zsh): draw the inline menu as a box under the word"
```

---

### Task 3: Docs for the box

**Files:**
- Modify: `docs/features/zsh-adapter.md`, `docs/features/popup.md`, `docs/design-system/README.md:40-50`, `docs/docs/12-adr.md`

- [ ] **Step 1: Feature specs**

In `docs/features/zsh-adapter.md`: add `display.inlineStyle` to `settings:` and `GOH_INLINE_STYLE` to `env:`, set `updated:` to today, add under `## Behaviour`:

```markdown
- Where the popup cannot draw (Ghostty, Kitty, WezTerm, tmux, ssh, ...), the menu is a floating framed box under the word you
  are completing, in the terminal's own colours; no permission is needed. `display.inlineStyle: "flat"` draws plain
  rows instead, which a screen reader reads more easily.
```

and under `## Changes`, first line:

```markdown
- unreleased: the inline menu is a floating framed box under the word being completed (`display.inlineStyle`, `box` by default; `flat` is the old look).
```

In `docs/features/popup.md`, change the first `## Limits` bullet to:

```markdown
- Inline in tmux, screen, native Wayland, unverified terminals: there the menu is a floating framed box drawn by the shell
  (see [zsh-adapter](zsh-adapter.md)).
```

and add `- unreleased: where the popup cannot draw, zsh draws a floating framed box instead.` under `## Changes`; set `updated:`.

- [ ] **Step 2: Design system**

In `docs/design-system/README.md`, replace the sentence `It is flat: no fill, no border.` with:

```markdown
By default it is a floating box: a one-cell frame in `text-muted`, filled with `surface-base`, the selected row in
`surface-selected`, starting under the first letter of the word being completed and moved left only as far as the
terminal's width needs. `display.inlineStyle: "flat"` keeps the frameless rows for screen readers. The frame is shape,
not colour: it stays under `NO_COLOR`.
```

- [ ] **Step 3: Decision record**

Append to `docs/docs/12-adr.md`, before `## ADR template for future records`:

```markdown
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
```

- [ ] **Step 4: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: no output, exit 0.

```bash
git add docs/features/zsh-adapter.md docs/features/popup.md docs/design-system/README.md docs/docs/12-adr.md
git commit -m "docs: describe the boxed inline menu"
```
