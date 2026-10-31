---
id: zsh-adapter
title: zsh: menus as you type
status: stable
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [zsh]
settings: [display.live, display.maxSuggestions, display.mode]
env: [GOH_LIVE, GOH_LIMIT, GOH_DISPLAY]
---

# zsh: menus as you type

In zsh the menu opens and follows what you type, with ghost text for the best line; Tab, arrows and Enter work as
in an IDE. Load it with `goh setup` or `eval "$(goh init zsh)"`.

## Behaviour

- As you type (`display.live`, on): after each edit of a recognised command the menu opens or updates; it closes
  when nothing matches. Esc hides it until the line changes.
- Tab/→ insert the highlighted candidate; ↑↓ and Shift+Tab move, wrapping; Esc closes; Left closes and moves.
  Navigation keys are bound only while the menu is open.
- Enter inserts the highlighted candidate after you moved in the menu, or the only one when there is just one;
  otherwise it runs the line as typed. Ctrl+C closes the menu at the next prompt.
- Ghost text: the best line is shown after the cursor; → takes it.
- Fail-open: any problem falls through to the widget Tab had before; `goh-unload` restores it.
- Works next to zsh-autosuggestions and plugins that replace `zle-line-init`.

## Limits

- Needs zsh; the menu is drawn by [popup](popup.md) where the caret is verified, else inline.

## Changes

- 1.10.0: Enter accepts a lone suggestion.
- 1.4.0: inline menu with kind glyphs, warnings on every row and a detail line.
- 1.4.1, 1.5.0, 1.6.0: zsh-autosuggestions and plugin widget fixes.
- 1.0.0: first release.
