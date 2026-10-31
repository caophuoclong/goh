---
id: other-shells
title: fish, elvish, xonsh
status: experimental
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [fish, elvish, xonsh]
commands: [goh init]
---

# fish, elvish, xonsh

Adapters for fish, elvish and xonsh complete through each shell's own completion hook.

## Behaviour

- `goh init <shell>` prints the adapter; `goh setup` adds fish's to `config.fish`.

## Limits

- Completion on the shell's own key only; no popup.

## Changes

- 1.0.0: first release.
- unreleased: the fish adapter is tested in a real fish.
- unreleased: fish completion produced no candidates because the adapter measured byte offsets with an option `string length` does not have; fixed.
- unreleased: PowerShell has its own spec (powershell-adapter).
- unreleased: Nushell has its own spec (nushell-adapter).
