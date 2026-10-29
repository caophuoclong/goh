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
- Never executes anything for you: it only inserts the candidate you pick.
- Runs fully local. Command text is not sent over the network (the one optional request is the update check, see [Privacy](#privacy)).

## Privacy

Everything runs locally. Three things are **off unless you turn them on** in `goh config`:

- `privacy.recordCommands`: keeps the successful lines you run in a private file (`~/.local/share/goh/runs`, mode
  0600) to finish a started line, predict the next command at an empty prompt and remember option values. It also
  reads your zsh history (`$HISTFILE` if exported, else `~/.zsh_history`) for what you ran before, with the same
  filters; that file is only read, never changed. Lines that
  look like they hold a secret (`password`, `token`, `user:pass@host`, long opaque keys) and lines starting with a
  space are never written. `goh forget` deletes it.
- `completion.autoSpecs`: the first time you type a program with no spec, runs `<program> --help` once in the
  background and saves a spec read from it (the same as `goh spec gen`).

- `updates.check`: once a day goh runs `curl` to read the version number of the latest release from GitHub's public
  release page, and prints one line when a shell starts if a newer one exists (`goh update` does the same once,
  whenever you ask). It is the only network request goh makes. Nothing about you, your machine or your commands is
  sent beyond what any web request carries (your IP address and a `goh/<version>` user agent).

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
a new shell: at its first menu macOS asks for **goh-popup** itself. The builds are not signed, so after each goh update
macOS treats goh-popup as a new program: switch it off and on again under Privacy & Security > Accessibility.
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

Packages exist for macOS (Apple silicon and Intel) and Linux (x86-64 and ARM). The macOS ones include the overlay popup;
the Linux ones use the inline menu.

The install script downloads the package for your system, checks its SHA-256, puts `goh`, `gohd` and `goh-popup` in
`~/.local/bin` (`GOH_BIN` changes it) and adds one marked block to your `~/.zshrc`, `~/.bashrc` or fish `config.fish`
that puts that folder on `PATH` and loads the adapter. `--version 1.2.3` picks a release; re-running updates;
`--uninstall` removes the binaries and exactly that block; `--no-rc` leaves your shell files alone.

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

Version 1.0. Verified on macOS with zsh and bash; Linux terminals (inline menu only, no overlay) and PowerShell are not verified yet.

```bash
cargo test
```

## License

Free and open source under the [MIT License](LICENSE).
