---
id: ssh-host-copy
title: goh for hosts that have none
status: experimental
since: unreleased
updated: 2026-10-31
area: ssh
shells: [zsh]
platforms: [macos, linux]
commands: [goh ssh]
env: [GOH_RELEASES, GOH_SOCKET, GOH_VISITS, GOH_USAGE, GOH_RUNS]
files: [~/.local/share/goh/ssh-hosts, ~/.cache/goh/ssh/, host: ~/.local/share/goh/ssh/<version>]
---

# goh for hosts that have none

When an ssh host's login shell is zsh and it has no goh, `goh ssh` offers to copy this version there, to keep or
for one session only, so its menus work without installing anything by hand.

## Behaviour

- One control master serves the session, so you sign in once. A probe (read only) reports the host's system,
  login shell, its own goh and a temporary folder that may run programs.
- The question is highlighted (red and bold; plain with `NO_COLOR`) and asked once per destination:
  - **k** keep a copy in `~/.local/share/goh/ssh/<version>`, sent once;
  - **s** this session only: a private folder in `$XDG_RUNTIME_DIR` (else `/tmp`), sent each time; its socket,
    log and data files stay in it, and it is removed with its daemon when the shell ends or the connection drops.
    Not offered when no temporary folder may run programs;
  - **N** no, plain ssh: the default for Enter, no answer, no terminal or anything else.
  The answer is kept in `~/.local/share/goh/ssh-hosts`; deleting its line asks again.
- The package for the host's system is downloaded once from this version's release, checked against
  `SHA256SUMS`, kept in `~/.cache/goh/ssh`, and sent gzipped over the same connection (the host needs no internet).
- The copy is loaded for the session only: zsh starts with a ZDOTDIR that reads your `.zshenv`, `.zprofile` and
  `.zshrc` first and then goh's adapter; `ZDOTDIR` is yours again from `.zshrc` on. No file of yours changes.
- A host with its own goh uses it; one too old for the relay gets a one-line note.

## Limits

- zsh hosts on Linux or macOS, x86-64 or ARM. Others, and failures, are plain ssh.
- A version with no release (a local build) needs its binaries in `~/.cache/goh/ssh/<version>/<target>/`.
- Without goh on the host there are no suggestions at all: they are computed where the shell runs.

## Changes

- unreleased: first version, with keep, session-only and no.
