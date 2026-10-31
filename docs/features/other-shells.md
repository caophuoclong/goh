---
id: other-shells
title: fish, PowerShell, Nushell, elvish, xonsh
status: experimental
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [fish, pwsh, nu, elvish, xonsh]
commands: [goh init]
---

# fish, PowerShell, Nushell, elvish, xonsh

Adapters for fish, PowerShell, Nushell, elvish and xonsh complete through each shell's own completion hook.

## Behaviour

- `goh init <shell>` prints the adapter; `goh setup` adds fish's to `config.fish`.

## Limits

- Completion on the shell's own key only; no popup.

## Changes

- 1.0.0: first release.
