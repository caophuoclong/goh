# zsh adapter (spike)

Tab-triggered inline completion through `gohd`. It does not hook per-keystroke redraws (docs/16, step 5).

| Key | Behaviour |
|---|---|
| Tab | One candidate: insert it. Several: open the inline menu (dim ghost text at end of line + list below). |
| Tab (menu open) | Insert the highlighted candidate. Never runs the command. |
| Up / Down, Shift+Tab | Move the highlight, wrapping. Bound **only while the menu is open**. |
| Esc | Close the menu. Bound only while it is open. |
| Enter | Shell default; the menu is dropped, nothing is inserted. |
| any edit / cursor move | Closes the menu. |

Ghost text appears only when the cursor is at the end of the line and the insertion extends what is typed;
otherwise just the list is shown. Any failure (no daemon, timeout, invalid span) falls through to the widget
Tab was bound to before. `goh-unload` restores it.

```bash
cargo build                                   # builds gohd and goh-ctl into target/debug
mkdir -p ~/.config/goh/specs && cp docs/examples/git.completion.json ~/.config/goh/specs/git.json
export PATH="$PWD/target/debug:$PATH"
source adapters/zsh/goh.zsh                   # after compinit
```

If the daemon is not running (first use, or it died and left a stale socket), `goh-ctl` starts it, waits up to 0.5 s for it to accept connections and asks once more, so the first Tab is not lost; if it cannot start, Tab falls through to the original widget after that bounded wait.
Inspect candidates as JSON: `goh-ctl complete --session zsh-demo-0001 --cwd . --cursor-byte 6 --buffer "git ch"`.
`GOH_LIMIT` sets the menu size (default 8).

Socket: `$XDG_RUNTIME_DIR/goh/gohd.sock` or `/tmp/goh-<uid>/gohd.sock` (dir 0700, socket 0600); override with `GOH_SOCKET`
(Unix socket paths must stay under ~104 bytes).

## Theme

On load (and on `goh-theme-refresh`) the adapter runs `goh-ctl theme --probe`, which asks the terminal for its
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
  private temp directory; the popup's answer is read asynchronously (`zle -F`), so a slow, missing or dead popup
  never blocks typing: the inline menu is drawn immediately and hidden only once the popup says it is drawing.
- iTerm2's cell height is asked once at load, between prompts (`goh-ctl theme --probe`), and passed to the popup,
  because a reply requested while ZLE is reading the terminal would be taken as typing.
- Per-item match ranges, the resolved theme tokens and the command name (footer) are included in each frame.

## Nothing shows?

Run `goh-status` at a prompt. It prints one line per layer: whether Tab is bound to the adapter, whether `goh-ctl`
and `goh-popup` are on `PATH`, whether `gohd` is running, which completion specs it can load, what a real query for
`git c` returns, whether the popup is running and what it last answered, and the gate's verdict for this terminal.
Typical causes: no spec in `~/.config/goh/specs` (nothing to complete), `goh-ctl` not on `PATH`, the first Tab after a
cold start (it only launches `gohd`; press Tab again), or an overlay that answers "inline" because the terminal has no
Accessibility access. `GOH_DEBUG=1` keeps the popup's stderr in a file whose path `goh-status` prints.

## As you type

With `GOH_LIVE=on` (the default; `off` = Tab only) the menu opens and updates by itself while you type, for a line
that is **valid**: the first word is a command with a completion spec, and every word before the one you are typing
fits that spec (a subcommand that exists, or a value an option or the command's arguments can take). When the line
stops being valid, or nothing matches, the menu closes; deleting back reopens it. `git c` and `git ch` open it,
`git foo c`, `zzz c` and `gi` do not (Tab still completes unknown commands as before). Esc hides it until the next
keystroke; accepting with Tab does not reopen it by itself. It reacts to typing and deleting, not to history recall
(Up), and when more input is queued (paste, key repeat) it waits and refreshes once. Each refresh is one query (about
3 ms measured), so it costs nothing you can feel. Rules and limits of "valid": docs/docs/06-completion-engine.md.
