---
id: bash-adapter
title: bash: completion on Tab, a menu as you type with ble.sh
status: stable
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [bash]
settings: [display.live]
env: [GOH_LIVE]
---

# bash: completion on Tab, a menu as you type with ble.sh

In bash, goh completes through bash's own programmable completion: press Tab. With
[ble.sh](https://github.com/akinomyoga/ble.sh) loaded (bash 4+), goh's candidates appear in ble.sh's menu with their
descriptions, and the menu opens by itself while you type.

## Behaviour

- Commands with a spec and no existing completion are registered with `complete -F`; Tab, the list and filename
  fallback stay bash's own. Commands that already have a completion (bash-completion's `git`) are left alone.
- Under ble.sh (source it before goh, e.g. `source ble.sh --attach=none` near the top of `~/.bashrc`): one candidate
  per line with its description; the menu opens 300 ms after you stop typing and narrows as you type.
  `display.live: false` keeps it on Tab. goh sets ble.sh's `complete_menu_style` and `complete_auto_menu` only
  while they are at ble.sh's defaults; your own values stay. The auto menu is ble.sh's, so it opens for every
  command, not only goh's.
- ble.sh calls goh's completion function for the menu, for Tab and for its own ghost text. Candidates with spaces or
  non-ASCII names are inserted quoted.
- With `privacy.recordCommands`, successful lines are recorded from `PROMPT_COMMAND`.

## Limits

- Without ble.sh: no menu as you type, no ghost text, no popup.
- Under ble.sh: no popup and no border; the menu is ble.sh's own list. Tested with bash 5.3 and ble.sh 0.4.0-devel3.

## Changes

- unreleased: under ble.sh, goh's candidates come with descriptions and the menu opens as you type.
- 1.0.0: first release.
