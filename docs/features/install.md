---
id: install
title: Install and setup
status: stable
since: 1.0.0
updated: 2026-10-31
area: tooling
platforms: [macos, linux]
commands: [goh setup, goh init]
env: [GOH_BIN, GOH_RC]
---

# Install and setup

One command installs goh and its shell integration; `goh setup` adds or removes exactly one marked block.

## Behaviour

- `install.sh` picks the package for the system, checks its SHA-256, installs to `~/.local/bin` (`GOH_BIN`) and
  adds one block to `~/.zshrc`, `~/.bashrc` or fish's `config.fish`; `--uninstall` removes exactly that.
- Homebrew: `brew install caophuoclong/goh/goh && goh setup`.
- `--from-source` builds with cargo; on macOS it installs the signed `Goh.app` like a release.

## Limits

- macOS and Linux, x86-64 and ARM packages.

## Changes

- unreleased: `--from-source` installs a signed `Goh.app` on macOS; `goh-popup.app` from earlier versions is removed.
- 1.8.0: popup in Linux source installs.
- 1.1.0: `goh setup`.
- 1.0.1: SHA256SUMS check.
