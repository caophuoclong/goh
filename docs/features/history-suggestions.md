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
- A line from your zsh history that looks secret is still offered while you type, with the secret shown as `****`
  in the menu, the popup and the ghost text (`export GITHUB_TOKEN=****`, `--password=****`, `Bearer ****`,
  `https://me:****@host`, long tokens); taking it inserts the line as zsh has it. goh keeps no copy of it.

## Limits

- Off by default. zsh only for ghost text and prediction.
- Masked lines are zsh only (the other shells show what they insert), never predicted, and never give option values.
  A line that looks secret with nothing goh can mask (`kubectl get secret web`) is not offered.

## Changes

- unreleased: lines with a secret from the zsh history are offered with the secret shown as `****`.
- 1.9.0: better prediction and history lines.
- 1.6.0: first release.
