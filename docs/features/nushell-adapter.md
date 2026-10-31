---
id: nushell-adapter
title: Nushell: a menu as you type
status: experimental
since: unreleased
updated: 2026-11-01
area: shells
shells: [nu]
commands: [goh init nu]
---

# Nushell: a menu as you type

In Nushell, goh adds a bordered menu next to the cursor that opens at each new word with goh's candidates and their
descriptions. In `config.nu`: `source /path/to/goh.nu`, then `goh-ui`.

## Behaviour

- Space opens the menu for the next word; typing narrows it and asks goh again; Down/Up (and Tab) move, Enter takes
  the highlighted candidate, Esc closes it and keeps the text.
- With `completions.persistent_menus` on, erasing keeps the menu open; `goh-ui` turns it on when your Nushell has the
  setting.
- Tab still completes through goh's external completer, as before (`goh-complete`).
- Both menu and Tab completion work with small shell process IDs, including in containers and fresh systems.
- Without `goh` on the path, or when it answers something unreadable, the menu stays empty and Space just types a space;
  Tab falls back to Nushell's own completion.
- Your own menus and key bindings stay; goh only appends `goh_menu` and a Space binding.

## Limits

- Opens at a space, not at every letter: Nushell opens menus only from a key binding.
- No popup at the caret; the box is Nushell's own IDE menu.
- With Nushell's `completions.partial` on (its default), opening the menu fills in the text all candidates share
  (`git ` can become `git c` when every candidate starts with c). goh leaves that setting as you have it.
- Tested with Nushell 0.116.1.

## Changes

- unreleased: menu and Tab completion no longer return nothing when the shell's process ID has fewer than five digits.
- unreleased: first release.
- unreleased: the Tab completer (`goh-complete`) did not load on Nushell 0.116 because `str length --bytes` was removed; it loads again.
- unreleased: a missing `goh` (or a malformed answer) no longer raises "External command failed" from Space or Tab; both give no candidates.
