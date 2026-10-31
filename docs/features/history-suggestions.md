---
id: history-suggestions
title: History lines and the next command
status: experimental
since: 1.6.0
updated: 2026-10-31
area: core
shells: [zsh, bash]
settings: [privacy.recordCommands, display.predict]
commands: [goh forget]
env: [GOH_RECORD, GOH_PREDICT, GOH_RUNS]
files: [~/.local/share/goh/runs]
---

# History lines and the next command

Opt in with `privacy.recordCommands`: goh finishes a started line from your history, predicts the command that
usually comes next at an empty prompt, and remembers the values you gave an option.

## Behaviour

- Kept in `~/.local/share/goh/runs` (0600); also reads your zsh history (read only).
- Offers the three lines you ran most, most recently and in this folder; not a line that did its job once.
- The best line is ghost text after the cursor; → takes it. `display.predict` shows the next command.
- Lines that look secret, and lines starting with a space, are never written. `goh forget` deletes the file.

## Limits

- Off by default. zsh only for ghost text and prediction.

## Changes

- 1.9.0: better prediction and history lines.
- 1.6.0: first release.
