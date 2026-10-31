# goh

**goh** is a from-scratch rebuild of [Fig](https://fig.io), the terminal autocomplete that showed
command suggestions right where you type. It brings IDE-style completion to your existing terminal: no new terminal, no account, no cloud.

It reproduces what Fig was useful for, not its branding, and has no dependency on Amazon Q CLI.

## What it does

- Suggests subcommands, flags, arguments and paths as you type, with descriptions.
- Is driven by declarative JSON completion specs (114 bundled: `git`, `docker`, `npm`, `pnpm`, `cargo`, `kubectl`, `just`, `task`, `brew`, `aws`, `deno`, `gradle`, `flutter`, `jq`, `tmux`, `systemctl`, ...; a command with none gets one from its `--help`, see `goh spec gen`).
- Completes the command after `sudo`, `time`, `env`, `xargs`, `watch`, `nohup`, `npx` and `FOO=1` by that command's own spec, skips options you already gave, reads `ls -la` as `-l -a` and offers `-` for the previous branch or directory.
- Shows an overlay popup anchored near the cursor, with an inline fallback where the terminal can't support one.
- Inherits your terminal's colors instead of imposing its own theme.
- Knows your project: git branches, tags, stashes, recent commits, changed files and your `[alias]`es (`git co`),
  `package.json` scripts and `node_modules/.bin`, `pyproject.toml`/`Pipfile` scripts, `deno.json` tasks,
  Makefile/justfile/Taskfile targets, compose services, Cargo bins/examples/features and workspace packages, `$CDPATH`
  folders, kubectl pods and namespaces (asked in the background), `$VARIABLES`.
- Corrects a slip (`git chekcout` finds `checkout`), narrows files by type (`python <TAB>` lists `.py`), flags risky
  options (`⚠ overwrites the remote branch`) and says which kubectl context or Terraform workspace they act on.
- Expands your own snippets (`snippets.json`) and, if you opt in, finishes lines from your history (the three you ran
  most, most recently and in this folder; not a line that did its job once, such as `brew install jq` or
  `git checkout -b fix`), shows the command that usually comes next, and remembers the values you gave an option (see
  [Privacy](#privacy)). The best of those lines is shown as ghost text after the cursor, as zsh-autosuggestions does:
  → takes the whole line, Tab the selected suggestion.
- Enter takes the highlighted suggestion once you have moved in the menu (↑↓), or the only one when the menu shows
  just one, and runs the line; an option that wants its value, or a folder, is only inserted. With several untouched
  suggestions, Enter runs the line as typed. `display.enterRuns: false` makes Enter only insert (a second Enter runs).
- Never executes anything for you: it only inserts the candidate you pick.
- Runs fully local. Command text is not sent over the network (the one optional request is the update check, see [Privacy](#privacy)).

## Privacy

Everything runs locally. Three things are **off unless you turn them on** in `goh config`:

- `privacy.recordCommands`: keeps the successful lines you run in a private file (`~/.local/share/goh/runs`, mode
  0600) to finish a started line, predict the next command at an empty prompt and remember option values. It also
  reads your zsh history (your shell's `$HISTFILE`, else `~/.zsh_history`) for what you ran before, with the same
  filters; that file is only read, never changed. Lines that
  look like they hold a secret (`password`, `token`, `user:pass@host`, long opaque keys) and lines starting with a
  space are never written. `goh forget` deletes it.
- `completion.autoSpecs`: the first time you type a program with no spec, runs `<program> --help` once in the
  background and saves a spec read from it (the same as `goh spec gen`).

- `updates.check`: once a day goh runs `curl` to read the version number of the latest release from GitHub's public
  release page, and prints one line when a shell starts if a newer one exists (`goh update` does the same once,
  whenever you ask). Nothing about you, your machine or your commands is sent beyond what any web request carries
  (your IP address and a `goh/<version>` user agent).

The only other request is the one you answer yes to in `ssh` (below): downloading this version's package for the
host's system from the same release page, once, checked against its `SHA256SUMS`.

Your snippets go in `~/.config/goh/snippets.json`: `[{"name":"gcm","template":"git commit -m \"{{cursor}}\""}]`.

## Troubleshooting

```bash
goh doctor            # what works and what does not, with the fix for each
goh doctor --fix      # start or restart the daemon, open the macOS Accessibility settings
goh doctor --test     # also ask the daemon to complete `git chec` and time the answer
goh doctor --report   # a summary to paste into an issue: versions and checks, your home shown as ~, no command text
goh logs              # the daemon's own log (starts, spec warnings, panics); it never holds what you typed
goh specs --missing   # programs you ran that have no spec (needs privacy.recordCommands)
```

`doctor` says whether this shell loaded the adapter, whether your startup file has the block `goh setup` writes, and
whether the running daemon is the same version as `goh` (an upgrade can leave the old one running: `goh doctor --fix`).
In an iTerm2, Terminal.app or VS Code terminal that lacks the Accessibility permission, the first menu of a zsh
session prints a one-line note instead of silently drawing inline.

macOS gives the permission to the terminal app the popup runs in, so each terminal is allowed on its own. To allow
goh once instead, for every terminal (experimental), set `"popup": { "ownPermission": true }` with `goh config` and open
a new shell: at its first menu macOS asks for **Goh** (the popup) itself. The macOS packages ship it as `Goh.app`, signed
with goh's own certificate, so the permission stays through updates. A build from source keeps it too when it is
signed with that certificate: `./install.sh --from-source` and `scripts/package.sh` use it by themselves when
`../macos-sign/goh-signing.p12` sits beside the checkout (or `GOH_MACOS_SIGN_P12` names it), and after a plain
`cargo build` run `scripts/macos-sign.sh target/debug/goh-popup`. Otherwise it is signed ad hoc and is a new program
to macOS after each rebuild: remove its old **Goh** (or **goh-popup**) entries under Privacy & Security > Accessibility (−) and
allow it again.
`goh doctor` still reports the terminal's own permission.

## Layout

| Path | Purpose |
|---|---|
| `crates/goh-core` | Spec model, parser and completion engine, plus the bundled `specs/` |
| `crates/goh-daemon` | `gohd` background daemon and the `goh` client (Unix socket IPC) |
| `crates/goh-popup` | Native overlay popup UI |
| `crates/goh-anchor` | Finds the cursor position for popup placement |
| `crates/goh-theme` | Terminal color detection |
| `crates/goh-shells` | Shell integration helpers |
| `adapters/zsh`, `adapters/bash` | Shell adapters |
| `tools/import-specs` | Imports Fig-style specs into goh's format |
| `docs/` | Vision, PRD, architecture, design system, spike results |

## Install

Pick one. None needs the source.

| How | Command |
|---|---|
| Install script (macOS, Linux) | `curl -fsSL https://raw.githubusercontent.com/caophuoclong/goh/main/install.sh \| sh` |
| Homebrew | `brew install caophuoclong/goh/goh && goh setup` |
| By hand | download `goh-<version>-<target>.tar.gz` from the [releases](https://github.com/caophuoclong/goh/releases), check it against `SHA256SUMS`, put `bin/*` on your `PATH` |
| From source | `cargo install --git https://github.com/caophuoclong/goh goh-daemon goh-popup`, or `./install.sh --from-source` |

Packages exist for macOS (Apple silicon and Intel) and Linux (x86-64 and ARM). Both include `goh-popup`. Linux can use a verified X11/AT-SPI caret path;
unsupported terminals and native Wayland use inline. See [Linux popup support](docs/linux-popup.md).

The install script downloads the package for your system, checks its SHA-256, puts `goh`, `gohd` and `goh-popup` in
`~/.local/bin` (`GOH_BIN` changes it) and adds one marked block to your `~/.zshrc`, `~/.bashrc` or fish `config.fish`
that puts that folder on `PATH` and loads the adapter. `--version 1.2.3` picks a release; re-running updates;
`--uninstall` removes the binaries and exactly that block; `--no-rc` leaves your shell files alone.

### Linux

The install script picks the x86-64 or ARM package by itself:

```bash
curl -fsSL https://raw.githubusercontent.com/caophuoclong/goh/main/install.sh | sh
```

Then open a new terminal. Pass options after `sh -s --`, for example a fixed release:

```bash
curl -fsSL https://raw.githubusercontent.com/caophuoclong/goh/main/install.sh | sh -s -- --version 1.8.0
```

Homebrew on Linux works too (`brew install caophuoclong/goh/goh && goh setup`). Suggestions work in any terminal;
the floating popup at the cursor needs X11 or XWayland, a terminal that exports `WINDOWID` and exposes AT-SPI
accessibility over D-Bus, and the `libxkbcommon-x11` and EGL/OpenGL libraries. Native Wayland, SSH, tmux and screen
use the inline menu. `goh-popup --probe` explains the choice; see [Linux popup support](docs/linux-popup.md).

Over SSH the popup works too: in a shell with the zsh adapter, `ssh` runs through `goh ssh` (`popup.overSsh` in
`goh config` turns that off; your own `ssh` alias or function always wins), and the remote zsh's menus are drawn
here, at the caret of the terminal you typed it in. One connection does everything, so you sign in once:
- A host with goh uses its own; it needs this version or newer, and its sshd must accept `LC_*` variables (Debian
  and Ubuntu do; the forwarded socket's path travels in `LC_GOH_POPUP`).
- A host whose login shell is zsh and that has no goh is offered a copy, in a highlighted question asked once per
  destination (the answer is kept in `~/.local/share/goh/ssh-hosts`; delete its line to be asked again):
  **k** keeps it in `~/.local/share/goh/ssh/<version>` there, sent once; **s** sends it for this session only, to a
  private folder in `$XDG_RUNTIME_DIR` (or `/tmp`) that is removed with everything goh wrote when you leave (a host
  whose temporary folders may not run programs gets no **s**); **N**, Enter or no answer is plain ssh. Either copy
  is loaded for the session only: your startup files on the host are read as usual and none of them is changed. The
  package is downloaded once for the host's system and kept in `~/.cache/goh/ssh`.
- `ssh host command`, `-N`, `-f` and the like are plain ssh, as are hosts with another login shell (bash only
  completes on Tab). Inside tmux or screen on the host the menu stays inline.

Homebrew and manual installs do not edit your shell files (Homebrew never does). Run once, then open a new terminal:

```bash
goh setup              # adds one marked block to ~/.zshrc, ~/.bashrc or fish's config.fish
goh setup --dry-run    # shows what it would write
goh setup --uninstall  # removes exactly that block
```

By hand instead: `eval "$(goh init zsh)"` in `~/.zshrc` after compinit (bash: `goh init bash`, fish: `goh init fish | source`).

Releases are versioned automatically from commit messages: see [CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md).

## Quick start (from source, without installing)

```bash
cargo build
export PATH="$PWD/target/debug:$PATH"

# zsh (after compinit)
source adapters/zsh/goh.zsh

# bash (after bash-completion)
source adapters/bash/goh.bash
```

Then type `git ch` and press Tab, or just keep typing in zsh, where live mode is on by default.
`gohd` starts on demand. Put your own specs in `~/.config/goh/specs/`.

See the [zsh](adapters/zsh/README.md) and [bash](adapters/bash/README.md) adapter READMEs for key
bindings and options, and [docs/docs/](docs/docs/00-vision.md) for design and status.

## Status

Version 1.0. Verified on macOS with zsh and bash; Linux X11 has automated VTE coverage; other Linux terminal configurations and PowerShell need further verification.

```bash
cargo test
```

## License

Free and open source under the [MIT License](LICENSE).
