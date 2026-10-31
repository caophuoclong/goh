---
id: bash-adapter
title: bash: completion on Tab
status: stable
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [bash]
---

# bash: completion on Tab

In bash, goh completes through bash's own programmable completion: press Tab.

## Behaviour

- Commands with a spec and no existing completion are registered with `complete -F`; Tab, the list and filename
  fallback stay bash's own. Commands that already have a completion (bash-completion's `git`) are left alone.
- With `privacy.recordCommands`, successful lines are recorded from `PROMPT_COMMAND`.

## Limits

- No menu as you type, no ghost text, no popup (in progress on a separate branch for bash 4.4+).

## Changes

- 1.0.0: first release.
