---
id: doctor
title: goh doctor and logs
status: stable
since: 1.0.0
updated: 2026-10-31
area: tooling
commands: [goh doctor, goh logs, goh status]
env: [GOH_LOG]
---

# goh doctor and logs

`goh doctor` says what works and what does not, with the fix for each; `goh logs` shows the daemon's own log.

## Behaviour

- Checks binaries, daemon (and its version against goh's), adapter in this shell and in the startup file, specs,
  terminal and popup path, Accessibility, config, auto specs, updates, zoxide.
- `--fix` starts or restarts the daemon and opens the Accessibility settings; `--test` times a real completion;
  `--report` prints a summary for an issue (home shown as `~`, no command text).
- A daemon nobody can reach any more (its socket removed or replaced) exits by itself.
- Over ssh it says whether menus reach the popup through [ssh-popup](ssh-popup.md).

## Limits

- The log never holds what you typed.

## Changes

- unreleased: a daemon whose socket was removed or taken over exits by itself within 2 s.
- unreleased: reports the `goh ssh` relay.
- 1.3.0: version check, `--test`, `--report`, `goh logs`.
