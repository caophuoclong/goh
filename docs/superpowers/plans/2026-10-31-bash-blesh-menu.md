# bash Menu as You Type with ble.sh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** bash users who run [ble.sh](https://github.com/akinomyoga/ble.sh) get goh's candidates in a menu with descriptions that opens by itself while they type, as zsh users do.

**Architecture:** Plain bash (readline) cannot show a menu as you type: `bind -x` handlers cannot hand control back to readline (`adapters/bash/goh.bash:3-7`). ble.sh is a line editor written in bash that replaces readline and already has what goh needs: a menu with one candidate and its description per line (`complete_menu_style=desc`), a menu that opens after a short pause in typing (`complete_auto_menu=<ms>`, comp_type `auto_menu`), and filtering as you keep typing (`menu-filter`). It still calls bash's programmable completion, so goh's existing `complete -F _goh_complete` function runs inside it. When ble.sh is attached, that function hands each candidate to ble.sh with `ble/complete/cand/yield mandb CAND DATA`, where `DATA` is `CAND<FS>SUFFIX<FS>TAIL<FS>DESCRIPTION` (`<FS>` = `$_ble_term_FS`, read from `ble/complete/action:mandb/*` in ble.sh 0.4.0-devel3, `lib/core-complete.sh:2829-2853`). The carapace project feeds ble.sh the same way. Without ble.sh nothing changes.

**Tech Stack:** bash 4+, ble.sh 0.4.0-devel3 (pinned in CI by SHA-256), Rust PTY tests.

**Spec:** `docs/features/bash-adapter.md`.

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits (`feat(bash): ...`); end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Plain bash, including macOS's bash 3.2, behaves exactly as today; the existing `bash_pty.rs` tests must pass unchanged.
- goh changes a ble.sh setting only while it is still at ble.sh's default (`complete_menu_style=align-nowrap`, `complete_auto_menu` empty); a value the user set stays. `display.live: false` (`GOH_LIVE=off`) leaves `complete_auto_menu` alone.
- ble.sh's own action quotes what it inserts, so goh yields unescaped values (`my file`, not `my\ file`).
- Fail-open: when ble.sh is missing, not attached or lacks `ble/complete/cand/yield`, candidates go to `COMPREPLY` as today (ble.sh reads that too, without descriptions).
- Before Task 2, the spike (Task 1) must pass on a real bash 4+ with ble.sh; if it fails, stop and report.

---

## File Structure

| File | Change |
|---|---|
| `adapters/bash/goh.bash` | Yield to ble.sh with descriptions; ble.sh defaults set at load |
| `crates/goh-daemon/tests/bash_blesh.rs` | bash 5 + ble.sh on a pseudo-terminal |
| `.github/workflows/ci.yml` | Install bash 5 and a pinned ble.sh on the macOS runner |
| `docs/features/bash-adapter.md` | Behaviour and limits |

---

### Task 1: Spike with a real ble.sh

**Files:** none (scratch only).

- [ ] **Step 1: Install bash 5 and ble.sh 0.4.0-devel3**

```bash
brew install bash
mkdir -p "$TMPDIR/blesh" && cd "$TMPDIR/blesh"
curl -fsSL -o blesh.tar.xz https://github.com/akinomyoga/ble.sh/releases/download/v0.4.0-devel3/ble-0.4.0-devel3-2.tar.xz
echo "bdcdcfff216495403adf82a701fe41675f64b64644dc77caabd3e015871ebd61  blesh.tar.xz" | shasum -a 256 -c -
tar -xJf blesh.tar.xz
ls ble-0.4.0-devel3/ble.sh
```

Expected: `blesh.tar.xz: OK` and the file exists.

- [ ] **Step 2: Try a yield with a description**

```bash
"$(brew --prefix)/bin/bash" --noprofile --norc -i
```

In that shell, paste:

```bash
source "$TMPDIR/blesh/ble-0.4.0-devel3/ble.sh" --attach=none
_spike() {
  if [[ ${BLE_ATTACHED-} ]]; then
    printf '%s\n' "comp_type=$comp_type" >> "$TMPDIR/spike.log"
    ble/complete/cand/yield.initialize mandb
    local fs=$_ble_term_FS c
    for c in checkout cherry-pick 'my file'; do
      ble/complete/cand/yield mandb "$c" "$c$fs$fs $fs about $c"
    done
  fi
}
complete -F _spike spike
bleopt complete_menu_style=desc complete_auto_menu=300
ble-attach
```

Type `spike ch` and wait. Expected: after about 300 ms a menu opens under the line with `checkout  about checkout` and `cherry-pick  about cherry-pick`; typing `e` narrows it to `checkout`; Enter or Tab inserts `checkout ` with one trailing space. Then type `spike my` and accept: the line must read `spike my\ file ` or `spike 'my file' ` (ble.sh quoted it), not `my\\ file`.

Record for Task 2:

- whether `ble/complete/cand/yield.initialize mandb` is needed (remove it and retry);
- the `comp_type` values in `$TMPDIR/spike.log` for the auto menu, for Tab, and for the ghost-text auto-complete;
- whether `compopt -o ble/no-default` is needed to stop ble.sh adding file names next to the yielded candidates.

If no menu with descriptions appears, stop and report what showed instead.

---

### Task 2: Yield goh's candidates to ble.sh

**Files:**
- Modify: `adapters/bash/goh.bash`
- Test: `crates/goh-daemon/tests/bash_blesh.rs`

**Interfaces:**
- Consumes: `goh menu` lines (`start<US>end<US>cursor<US>kind<US>label<US>insert<US>description<US>ranges...`), as today.
- Produces: `_goh_unescape VALUE` (sets `REPLY` to `VALUE` without shell backslash escapes); `_goh_ble_setup CONFIG_ENV` (adjusts ble.sh defaults at load); in `_goh_complete`, one `ble/complete/cand/yield mandb` per candidate when ble.sh is attached, `COMPREPLY` otherwise.

- [ ] **Step 1: Write the failing test**

`crates/goh-daemon/tests/bash_blesh.rs`:

```rust
//! adapters/bash/goh.bash under ble.sh, in a bash 4+ on a pseudo-terminal: goh's candidates with their descriptions in
//! ble.sh's menu, opened by itself while typing. Skipped (with a note) without bash 4+, ble.sh or a UTF-8 locale.
//! ble.sh is found through GOH_TEST_BLESH (its ble.sh file) or ~/.local/share/blesh/ble.sh.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::{utf8_locale, Fixture, Pty};

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/bash/goh.bash")
}

/// A bash of version 4 or newer: macOS's /bin/bash is 3.2, Homebrew's is not.
fn bash4() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.map(|d| d.join("bash")).filter(|b| b.is_file()).find(|b| {
        Command::new(b).args(["-c", "echo ${BASH_VERSINFO[0]}"]).output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok())
            .is_some_and(|v| v >= 4)
    })
}

fn blesh() -> Option<PathBuf> {
    std::env::var_os("GOH_TEST_BLESH").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/blesh/ble.sh")))
        .filter(|p| p.is_file())
}

/// What the terminal shows: `out` without CSI and OSC sequences.
fn screen(out: &[u8]) -> String {
    let s = String::from_utf8_lossy(out);
    let (mut text, mut chars) = (String::new(), s.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\x1b', Some('[')) => {
                chars.next();
                while chars.next().is_some_and(|c| !('@'..='~').contains(&c)) {}
            }
            ('\x1b', Some(']')) => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => text.push(c),
        }
    }
    text
}

fn shell(fx: &Fixture, bash: &Path, ble: &Path, env: &[(&str, &str)]) -> Pty {
    let mut sh = Pty::spawn(fx.command(bash.to_str().unwrap(), &["--noprofile", "--norc", "-i"], env));
    sh.drain(Duration::from_millis(400), Duration::from_secs(3));
    sh.send(&format!("PS1='READY> '; source '{}' --attach=none\n", ble.display()), 1500);
    sh.send(&format!("source '{}'\n", adapter().display()), 800);
    sh.send("ble-attach\n", 1500);
    sh.drain(Duration::from_millis(800), Duration::from_secs(10)); // the first load fills ble.sh's cache
    sh
}

#[test]
fn the_menu_opens_by_itself_with_descriptions() {
    let (Some(bash), Some(ble), Some(locale)) = (bash4(), blesh(), utf8_locale()) else {
        eprintln!("skipping: needs bash 4+, ble.sh and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = shell(&fx, &bash, &ble, &[]);
    sh.send("git c", 0);
    let shown = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(shown.contains("checkout"), "the menu lists git's subcommands without Tab: {shown}");
    assert!(shown.contains("Switch branches"), "with goh's descriptions: {shown}");
}

#[test]
fn with_live_off_the_menu_waits_for_tab() {
    let (Some(bash), Some(ble), Some(locale)) = (bash4(), blesh(), utf8_locale()) else {
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = shell(&fx, &bash, &ble, &[("GOH_LIVE", "off")]);
    sh.send("git c", 0);
    let idle = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(!idle.contains("Switch branches"), "no menu before Tab: {idle}");
    sh.send("\t", 0);
    let tabbed = screen(&sh.drain(Duration::from_millis(1200), Duration::from_secs(5)));
    assert!(tabbed.contains("Switch branches"), "Tab opens it, with descriptions: {tabbed}");
}
```

- [ ] **Step 2: Run it to see it fail**

```bash
export GOH_TEST_BLESH="$TMPDIR/blesh/ble-0.4.0-devel3/ble.sh"
cargo test -p goh-daemon --test bash_blesh
```

Expected: `the_menu_opens_by_itself_with_descriptions` FAILS (no menu without Tab, and no descriptions). `with_live_off_the_menu_waits_for_tab` FAILS on the description after Tab.

- [ ] **Step 3: Yield to ble.sh in `_goh_complete`**

In `adapters/bash/goh.bash`, add above `_goh_complete`:

```bash
# A word as goh escapes it for the shell (`my\ file`), without the escapes: ble.sh quotes what it inserts itself.
_goh_unescape() {
  local s=$1 out= c
  while [[ $s ]]; do
    c=${s:0:1} s=${s:1}
    if [[ $c == '\' && $s ]]; then c=${s:0:1} s=${s:1}; fi
    out+=$c
  done
  REPLY=$out
}
```

In `_goh_complete`, replace the loop at the end:

```bash
  local IFS=$'\n'
  for line in $out; do
    IFS=$'\x1f' read -r start end cursor kind label insert desc ranges <<<"$line"
    # Only candidates that replace exactly the word being completed and extend what is typed.
    [[ $start == $(( point - cur_bytes )) && $end == "$point" ]] || continue
    [[ $insert == "$cur"* ]] || continue
    COMPREPLY+=("$insert")
  done
}
```

with:

```bash
  # Under ble.sh, candidates go to its menu with their descriptions (its `mandb` action: CAND, menu suffix, the tail
  # added after insertion, description, separated by $_ble_term_FS). Elsewhere, and if that fails, COMPREPLY.
  local ble= fs=${_ble_term_FS-}
  [[ ${BLE_ATTACHED-} && $fs ]] && declare -F ble/complete/cand/yield >/dev/null && ble=1
  [[ $ble ]] && ble/complete/cand/yield.initialize mandb
  local IFS=$'\n' tail value
  for line in $out; do
    IFS=$'\x1f' read -r start end cursor kind label insert desc ranges <<<"$line"
    # Only candidates that replace exactly the word being completed and extend what is typed.
    [[ $start == $(( point - cur_bytes )) && $end == "$point" ]] || continue
    [[ $insert == "$cur"* ]] || continue
    if [[ $ble ]]; then
      tail=; [[ $insert == *' ' ]] && tail=' '
      _goh_unescape "${insert%' '}"; value=$REPLY
      # Its status is not checked: ble.sh returns non-zero for a candidate its own filter drops.
      ble/complete/cand/yield mandb "$value" "$value$fs$fs$tail$fs${desc:-$kind}"
    else
      COMPREPLY+=("$insert")
    fi
  done
}
```

Apply what the spike recorded: drop the `yield.initialize` line if ble.sh did not need it, and add `[[ $ble ]] && compopt -o ble/no-default` after the loop if file names showed up next to goh's candidates.

- [ ] **Step 4: Set ble.sh's defaults at load**

Add above `goh-load`:

```bash
# ble.sh loaded before this file: show descriptions in its menu and, with display.live on (GOH_LIVE, default on),
# open the menu by itself 300 ms after you stop typing. Only settings still at ble.sh's defaults change.
_goh_ble_setup() {
  local live=${GOH_LIVE-}
  if [[ -z $live ]]; then
    live=on; [[ $1 == *GOH_LIVE=off* ]] && live=off
  fi
  [[ ${bleopt_complete_menu_style-align-nowrap} == align-nowrap ]] && bleopt complete_menu_style=desc
  [[ $live == on && -z ${bleopt_complete_auto_menu-} ]] && bleopt complete_auto_menu=300
  return 0
}
```

In `goh-load`, after the `updates.check` line:

```bash
  [[ ${BLE_VERSION-} ]] && _goh_ble_setup "$cfg"
```

Update the header comment's first paragraph by adding, after `(fail-open).`:

```bash
# Under ble.sh (bash 4+, sourced before this file), the same function feeds ble.sh's menu with descriptions, and the
# menu opens by itself while you type (see _goh_ble_setup).
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p goh-daemon --test bash_blesh
cargo test -p goh-daemon --test bash_pty
```

Expected: both PASS; `bash_pty` runs the first `bash` on PATH (macOS 3.2) and is unchanged.

- [ ] **Step 6: Try it by hand**

In `"$(brew --prefix)/bin/bash" --noprofile --norc -i`: `source "$GOH_TEST_BLESH" --attach=none; source adapters/bash/goh.bash; ble-attach`, then type `docker run --`, `cd ` (in a folder with a space in a subfolder name), `kubectl get `. Expected: the menu opens after a short pause with descriptions; accepting a folder with a space inserts it quoted once; Esc closes the menu.

- [ ] **Step 7: Commit**

```bash
git add adapters/bash/goh.bash crates/goh-daemon/tests/bash_blesh.rs
git commit -m "feat(bash): a menu with descriptions as you type under ble.sh"
```

---

### Task 3: CI and the spec

**Files:**
- Modify: `.github/workflows/ci.yml`, `docs/features/bash-adapter.md`

- [ ] **Step 1: Install bash 5 and the pinned ble.sh in CI**

In job `test-macos`, add before `- run: cargo test --workspace --locked`:

```yaml
      - name: Install bash 5 and ble.sh for the ble.sh tests
        env:
          BLESH_TAG: v0.4.0-devel3
          BLESH_ASSET: ble-0.4.0-devel3-2.tar.xz
          SHA256: bdcdcfff216495403adf82a701fe41675f64b64644dc77caabd3e015871ebd61
        run: |
          brew install bash
          curl -fsSL -o "$RUNNER_TEMP/blesh.tar.xz" "https://github.com/akinomyoga/ble.sh/releases/download/${BLESH_TAG}/${BLESH_ASSET}"
          echo "${SHA256}  $RUNNER_TEMP/blesh.tar.xz" | shasum -a 256 -c -
          tar -xJf "$RUNNER_TEMP/blesh.tar.xz" -C "$RUNNER_TEMP"
          echo "GOH_TEST_BLESH=$RUNNER_TEMP/ble-0.4.0-devel3/ble.sh" >> "$GITHUB_ENV"
```

Run: `actionlint .github/workflows/ci.yml`
Expected: no output.

- [ ] **Step 2: Update the spec**

In `docs/features/bash-adapter.md`: change the title line to `title: bash: completion on Tab, a menu as you type with ble.sh`, add `settings: [display.live]` and `env: [GOH_LIVE]` to the front matter, set `updated:` to today, and replace the body from `# bash: completion on Tab` to `## Changes` with:

```markdown
# bash: completion on Tab, a menu as you type with ble.sh

In bash, goh completes through bash's own programmable completion: press Tab. With
[ble.sh](https://github.com/akinomyoga/ble.sh) loaded (bash 4+), goh's candidates appear in ble.sh's menu with their
descriptions, and the menu opens by itself while you type.

## Behaviour

- Commands with a spec and no existing completion are registered with `complete -F`; Tab, the list and filename
  fallback stay bash's own. Commands that already have a completion (bash-completion's `git`) are left alone.
- Under ble.sh (source it before goh, e.g. `source ble.sh --attach=none` near the top of `~/.bashrc`): one candidate
  per line with its description; the menu opens 300 ms after you stop typing and narrows as you type.
  `display.live: false` keeps it on Tab. goh sets ble.sh's `complete_menu_style` and `complete_auto_menu` only
  while they are at ble.sh's defaults; your own values stay. The auto menu is ble.sh's, so it opens for every
  command, not only goh's.
- With `privacy.recordCommands`, successful lines are recorded from `PROMPT_COMMAND`.

## Limits

- Without ble.sh: no menu as you type, no ghost text, no popup.
- Under ble.sh: no popup and no border; the menu is ble.sh's own list. Tested with ble.sh 0.4.0-devel3.
```

Add under `## Changes`, first line:

```markdown
- unreleased: under ble.sh, goh's candidates come with descriptions and the menu opens as you type.
```

- [ ] **Step 3: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add .github/workflows/ci.yml docs/features/bash-adapter.md
git commit -m "docs: describe bash with ble.sh; ci: test it with a pinned ble.sh"
```
