# zsh adapter (spike)

Completion through `gohd`, shown automatically while typing by default (`GOH_LIVE=on`). The menu
uses an overlay when the terminal supports it, with an inline fallback.

| Key | Behaviour |
|---|---|
| Tab (menu closed) | Native completion in live mode. With `GOH_LIVE=off`: insert a lone candidate or open the menu for several. |
| Tab / Right (menu open) | Insert the highlighted candidate, then close. Never runs the command. |
| Enter | After ↑↓/Shift+Tab or wheel navigation: insert the highlighted candidate without executing. Before navigation or with no menu: original shell binding, run the typed line. Editing resets navigation intent. |
| Wheel / trackpad over popup (macOS) | Move the highlight and scroll the list; stops at either end. Keyboard focus stays in the terminal. |
| Up / Down, Shift+Tab | Move the highlight, wrapping. Bound **only while the menu is open**. |
| Esc | Close the menu. Bound only while it is open. |
| Left | Close the menu and perform the original cursor movement. |
| Arrows (menu closed) | Original shell bindings. |
| Ctrl+C | Abort the line normally and close the popup when the next prompt starts. |
| typing / deleting | Refresh the live menu; close it if nothing matches. With `GOH_LIVE=off`, edits close it. |

Ghost text appears only when the cursor is at the end of the line and the insertion extends what is typed;
otherwise just the list is shown. On zsh 5.9+, its color is tagged with a native `memo` so refreshes and acceptance
remove only the adapter's highlighting, even after ZLE converts colors or moves character ranges.
At line start/finish, cleanup lets ZLE remove its message and redraw the prompt; forcing a message refresh
there can make the next multiline prompt lose its final line or symbol. Older zsh
shows the same ghost text without a custom color. Any failure (no daemon, timeout, invalid span) falls through to the widget
Tab was bound to before. `goh-unload` restores it.

```bash
cargo build                                   # builds gohd and goh into target/debug
export PATH="$PWD/target/debug:$PATH"
source adapters/zsh/goh.zsh                   # after compinit
```

If the daemon is not running (first use, or it died and left a stale socket), `goh` starts it, waits up to 0.5 s for it to accept connections and asks once more, so the first Tab is not lost; if it cannot start, Tab falls through to the original widget after that bounded wait.
Inspect candidates as JSON: `goh complete --session zsh-demo-0001 --cwd . --cursor-byte 6 --buffer "git ch"`.
`GOH_LIMIT` sets the visible row count (default 8, minimum 1). The adapter requests up to 50 candidates independently; arrows and scrolling can reach results beyond the first visible rows.

`goh reload` restarts `gohd` and, in this shell, loads the adapter again (from the installed `goh`, with the config
read again), so no `exec $SHELL` is needed there; other open shells keep their adapter until they do the same.

With zsh-autosuggestions loaded, both draw after the line (`POSTDISPLAY`), so while the menu is open the plugin is
paused, an answer it already had on the way is cancelled, and its keys that take a suggestion into the line (End,
Ctrl+E, Alt+F, `autosuggest-accept`, ...) close the menu first. It resumes when the menu closes, unless you had turned
it off yourself.

Socket: `$XDG_RUNTIME_DIR/goh/gohd.sock` or `/tmp/goh-<uid>/gohd.sock` (dir 0700, socket 0600); override with `GOH_SOCKET`
(Unix socket paths must stay under ~104 bytes).

## Theme

On load (and on `goh-theme-refresh`) the adapter runs `goh theme --probe`, which asks the terminal for its
default foreground/background (OSC 10/11) on `/dev/tty`: bounded to ~100 ms and 1 KiB, replies filtered, terminal
mode always restored. Run it between prompts; typeahead during that window is consumed. It is skipped under
`tmux`/`screen`, `TERM=dumb`, and when `theme.mode` is not `auto`.

`goh-theme` shows how it was resolved. `source` is `verified` (both colours), `approximate` (background only),
`env-hint` (`COLORFGBG`), `fallback` (design-system dark theme) or `override`; `adjusted` lists tokens moved to reach
4.5:1. Ghost text uses `text-muted`. Config lives in `~/.config/goh/config.json` (`GOH_CONFIG` overrides),
e.g. `{"theme": {"mode": "custom", "foreground": "#e6e6e6", "background": "#101418", "accent": "#7aa2f7"}}`.

## Overlay (goh-popup)

With `GOH_DISPLAY=auto` (the default) or `overlay`, the menu is also sent to `goh-popup`, which draws it at
the caret when the caret is verified (iTerm2 and Terminal.app, with Accessibility access granted to the
terminal; see docs/docs/18) and otherwise answers "inline", in which case nothing changes. `GOH_DISPLAY=inline`
never starts it. `GOH_POPUP` names the binary (default: `goh-popup` on `PATH`).

- The popup starts on first use, in the background, and exits when the shell does. Frames go over two FIFOs in a
  private temp directory; the popup's answer is read asynchronously by a ZLE widget (`zle -F -w`), so a slow,
  missing or dead popup never blocks typing. Startup waits for the popup without drawing a temporary inline list,
  so an overlay confirmation does not resize the terminal's message area or force another redraw. An inline
  decision enables the fallback; if the popup never answers, an asynchronous one-second deadline enables it.
  Once a display mode is known, repeated confirmations do not redraw the terminal. A timeout cannot reopen a
  menu that has already been dismissed, and a timeout after a successful answer is ignored.
- iTerm2's cell height is asked once at load, between prompts (`goh theme --probe`), and passed to the popup,
  because a reply requested while ZLE is reading the terminal would be taken as typing.
- On macOS, wheel input is routed only inside the visible popup bounds, using existing Accessibility
  authorization. Clicks still pass through, and keyboard focus stays in the terminal. The popup sends
  `{"type":"navigate","id":"<current-query-id>","delta":<rows>}`; the adapter validates the query id and
  bounded integer delta, clamps the selection at either end, and returns the confirmed state. Stale events
  cannot change a new query or a dismissed menu. Arrow keys continue to wrap. If the OS does not allow the
  wheel handler, `goh-status`'s last popup decision contains `"mouse_scroll":false`; arrows still work.
- Per-item match ranges, the resolved theme tokens and the command name (footer) are included in each frame.
- An open popup checks terminal visibility every 50 ms. A missing or minimised focused window is rejected before
  caret lookup; after the first successful lookup, Accessibility failures use a shorter timeout and skip startup
  retries. Caret stabilisation remains enabled to avoid position drift during redraws. Actual hide latency also
  includes the OS and window renderer; the interval is not a guarantee of a 50 ms total response.
- A verified popup retains its originating Accessibility window and terminal text element. A different tab,
  pane or window hides it instead of moving it to the new caret; returning to the owner can show it again while
  the menu is still open. Closing the menu clears this ownership. An initial inline-only result does not start
  a background watcher that could later show an unconfirmed overlay.

## Nothing shows?

Run `goh-status` at a prompt. It prints one line per layer: whether Tab is bound to the adapter, whether `goh`
and `goh-popup` are on `PATH`, whether `gohd` is running, which completion specs the next daemon start will load, what a real query for
`git c` returns, whether the popup is running and what it last answered, and the gate's verdict for this terminal.
Typical causes: a spec of yours in `~/.config/goh/specs` that replaces a bundled one with fewer commands (`goh-status` says so), `goh` not on `PATH`, a daemon that cannot
start, or an overlay that answers "inline" because the terminal has no
Accessibility access. `GOH_DEBUG=1` keeps the popup's stderr in a file whose path `goh-status` prints.

## As you type

With `GOH_LIVE=on` (the default; `off` = Tab only) the menu opens and updates by itself while you type, for a line
that is **valid**: the first word is a command with a completion spec, and every word before the one you are typing
fits that spec (a subcommand that exists, or a value an option or the command's arguments can take). When the line
stops being valid, or nothing matches, the menu closes; deleting back reopens it. Exact command names such as
`docker`, `git` and `npm` open their subcommands immediately, without a space. Accepting adds the separator along
with the selected word; typing the space yourself continues normal completion. `git c` and `git ch` also open it,
`git foo c`, `zzz c` and `gi` do not (Tab still completes unknown commands as before). Esc hides it until the next
keystroke. Tab does not open a closed popup in live mode; it uses native completion. Accepting with Tab or
Right does not reopen it by itself, except for a directory (`cd src/`): its contents are what comes next, so the menu
reopens for them. Ctrl+C dismisses the old menu at the next prompt without replacing signal traps.
It reacts to typing and deleting, including Oh My Zsh's `magic-space` binding, not to history recall
(Up), and when more input is queued (paste, key repeat) it waits and refreshes once. Each refresh is one query (about
3 ms measured), so it costs nothing you can feel. Rules and limits of "valid": docs/docs/06-completion-engine.md.

## Which commands complete

**40 command specs are bundled in the binary**: `git`, `docker`, `npm`, `node`, plus `pnpm`, `yarn`, `bun`,
`npx`, `brew`, `curl`, `wget`, `ssh`, `scp`, `rsync`, `ls`, `cd`, `cp`, `mv`, `rm`, `mkdir`, `rg`, `find`, `tar`,
`unzip`, `python`, `python3`, `pip`, `pip3`, `uv`, `cargo`, `rustup`, `go`, `kubectl`, `helm`, `terraform`, `aws`,
`gcloud`, `gh`, `just` and `task`. 36 commands, including the expanded Docker spec, use pinned, MIT-licensed Fig declarations converted to static JSON; coverage and
omissions are documented in `tools/import-specs/README.md` and `report.json`. Nothing needs installing to use them.
Put your own
`<command>.json` specs (schema: `docs/schemas/completion.schema.json`; an example: `docs/examples/git.completion.json`)
in `~/.config/goh/specs/` (or `$GOH_SPEC_DIR`) to add commands. A spec of yours **replaces** the bundled one for the
same command, so keep your own `git.json` only if you mean to. `goh-status` and `goh specs` list the configuration
for the **next daemon start**: each command, where it comes from and what it replaces. The running daemon may differ;
it reads specs only when it starts. Restart it after changing them (stop the `gohd` process; it starts again on the next
Tab). `goh specs` keeps its tab-separated rows on stdout and prints this notice on stderr.

Bundled specs offer static subcommands, options and supported file paths; project-specific values (git branches, tags,
stashes, changed files, scripts, make/just/task targets, compose services, Cargo targets, workspace packages) come
from providers that read the current project (docs/docs/06-completion-engine.md).

## Command record and prediction (opt-in)

Off by default. With `privacy.recordCommands` set in `goh config` (open shells pick it up after `exec $SHELL`) the
adapter notes each line when it starts and hands a **successful** one to `goh record`, which leaves out anything
that looks secret and lines starting with a space. Then:

- typing the start of a line you ran before adds it to the menu (and as ghost text when it is the top row);
- at an empty prompt the command that usually follows the last one run in this folder shows as ghost text; **Right**
  or **Tab** puts it in the line, any other key drops it (`display.predict` / `GOH_PREDICT=off` turns it off);
- `kubectl -n <TAB>` lists the values you gave `-n` before.

`goh forget` deletes the record. The Right arrow is rebound only while this is on, and passes through to your own
binding when no prediction is showing.

Docker now includes nested `container`, `image`, and `compose` commands and their flags. Empty argument
positions offer available flags as well as known values. Fixed read-only providers obtain running/all
container names and local image tags from Docker; queries are cached for two seconds, bounded to 120 ms
and 128 KiB of output, and fail open. An unavailable Docker daemon still leaves static flags available.
Explicit host/context/config overrides disable runtime values to avoid suggesting from the wrong target;
providers otherwise inherit the daemon's Docker environment (restart it after changing exported Docker variables).
Programs inside containers are opaque: goh does not run them or invent their arguments.

The native popup embeds Devicon logos for Docker, Node, Go, Git, npm/npx, Python tools, Rust tools,
Kubernetes/Helm, pnpm, Yarn, Terraform and GitHub CLI. Logos appear beside rows and command context,
with width reserved by the layout; they require no icon font or runtime download. Commands without a
branded logo use a muted `>_` terminal outline in the same slot.
