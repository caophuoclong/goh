# README demo

`features.gif` records the actual release build of goh, its daemon and the zsh adapter in a real pseudo-terminal.
[Charmbracelet VHS](https://github.com/charmbracelet/vhs) automates the keystrokes and captures the terminal.
`features.gif` is the main workflow; `demo.gif` is a separate tour of project-aware completion, snippets and
opt-in history. The README embeds each GIF once.

## Regenerate

Install Rust, Git, zsh and VHS with its `ttyd` and FFmpeg dependencies. On macOS:

```sh
brew install vhs ttyd ffmpeg
```

Then, from the repository root:

```sh
scripts/record-demo.sh
```

The script also works when invoked by its absolute path from another directory. It builds the current checkout
with `cargo build --release --locked -p goh-daemon`, creates a private directory under `/tmp`, starts a daemon
on a private socket, runs [demo.tape](demo.tape) and [tour.tape](tour.tape) in separate fixtures, verifies the
results, and replaces both GIFs only after success.
Do not invoke the tape directly: its hidden setup needs the environment created by the script.

The tape uses Catppuccin Mocha, Menlo at 22 px, a 1100 × 440 canvas, 25 fps and a steady cursor. Menlo ships
with macOS; another OS needs that font or an explicit change to `FontFamily` in the tape. Using the same font
and VHS version keeps the layout consistent (the checked-in GIF was recorded with VHS 0.12.1).

## Recorded workflow

1. Type `git chec`, watch the menu narrow, move from `check-ignore` to `checkout`, and insert it with Tab.
2. Type `feature/`, pick `feature/search` from real local branches with ↓ and Tab, then run the checkout.
3. Clear the screen; type `docker run --`, move through flags and insert the highlighted one with Tab.
4. Clear the line; type `git push --force` and pause on the red `⚠ overwrites the remote branch` warning.
5. Clear the line; type `git chekcout`, see `checkout` offered, and accept the correction with Tab.
6. Type `ls src/`, expand `components/` with →, select a child and insert the full path with Tab.
7. Clear the line and screen, returning to the initial prompt for the next loop.

The second tour shows scripts read from a real `package.json`, targets read from a real Makefile, expansion of
a `gcm` snippet with the cursor inside the message quotes, frequent commands from fixture zsh history, and a
placeholder `API_TOKEN` masked by goh. These lines are never submitted.

The only visible command submitted is a checkout in the disposable Git repository. Docker is neither required
nor invoked: goh reads its bundled completion spec. No force push runs. Suggestions, descriptions, warnings,
ghost text, selection and Git output all come from the running programs; nothing is drawn over the capture.

## Isolation and checks

Terminal children run with a clean environment, a temporary home and zsh startup files, private goh config,
state and socket, an empty custom-spec directory, and no global Git config. The fixture has an initial
commit, two real branches, source files, npm scripts and Makefile targets. It has no remote. Choice learning,
auto-generated specs and update checks are disabled. History is off in the main demo; only the tour enables it,
reading a purpose-built history file with a non-secret placeholder token. The script stops only its private
daemon and removes the fixture on exit or failure.

VHS waits for actual menu text and Git's branch-switch confirmation. The wrapper checks the final Git branch,
1100 px width, a 10–20 second duration and a 3 MB size budget before publishing. VHS's palette and frame-delta
encoding, plus a non-blinking cursor, keep the GIF small without speeding up the recording.

After changing the tapes or application, play both GIFs to check navigation, legibility, timing and the loop
boundary. To extract a frame for closer inspection, use FFmpeg, for example:

```sh
ffmpeg -ss 10 -i docs/media/features.gif -frames:v 1 target/demo-frame.png
```

This captures the supported **inline menu**, not the native floating popup. To demonstrate the macOS popup,
record a real terminal window with macOS screen capture so the separate overlay window is included.
