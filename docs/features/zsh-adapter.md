---
id: zsh-adapter
title: zsh: menus as you type
status: stable
since: 1.0.0
updated: 2026-10-31
area: shells
shells: [zsh]
settings: [display.live, display.maxSuggestions, display.mode, display.enterRuns, display.inlineStyle]
env: [GOH_LIVE, GOH_LIMIT, GOH_DISPLAY, GOH_ENTER_RUNS, GOH_INLINE_STYLE]
---

# zsh: menus as you type

In zsh the menu opens and follows what you type, with ghost text for the best line; Tab, arrows and Enter work as
in an IDE. Load it with `goh setup` or `eval "$(goh init zsh)"`.

## Behaviour

- As you type (`display.live`, on): after each edit of a recognised command the menu opens or updates; it closes
  when nothing matches. Esc hides it until the line changes.
- Tab/→ insert the highlighted candidate; ↑↓ and Shift+Tab move, wrapping; Esc closes; Left closes and moves.
  Navigation keys are bound only while the menu is open.
- Esc closes at once, without zsh's `KEYTIMEOUT` wait: while the menu is open goh reads what follows Esc itself, and
  Alt+key, a paste or any other key starting with Esc closes the menu and does what it did before.
- Enter takes the highlighted candidate after you moved in the menu, or the only one when there is just one, and runs
  the line; an option that wants its value, or a folder, is only inserted. `display.enterRuns: false` makes Enter
  only insert (a second Enter runs). With several untouched candidates Enter runs the line as typed. Ctrl+C closes
  the menu at the next prompt.
- Ghost text: the best line is shown after the cursor; → takes it.
- Where the popup cannot draw (Ghostty, Kitty, WezTerm, tmux, ssh, ...), the menu is a floating framed box under the word you
  are completing, in the terminal's own colours; no permission is needed. `display.inlineStyle: "flat"` draws plain
  rows instead, which a screen reader reads more easily. The box finds the word's column by measuring your prompt,
  including one that uses `prompt_subst` parameters (oh-my-zsh themes, powerlevel10k, starship); row borders stay
  straight with wide characters (CJK, emoji).
- A program still answering (a slow Docker, a cluster) shows `…` in the footer; its values appear when they arrive. The
  wait stops when you close the menu (Esc, Left) or move in it, and gives up after about 5 s; each new prompt starts
  the count again, so the same line typed later is asked again.
- Fail-open: any problem falls through to the widget Tab had before; `goh-unload` restores it.
- Works next to zsh-autosuggestions and plugins that replace `zle-line-init`.

## Limits

- Needs zsh; the menu is drawn by [popup](popup.md) where the caret is verified, else inline.
- A prompt that runs a command (`$(...)` or a backtick under `prompt_subst`) is not run again to measure it: the box
  anchors as if the prompt had no width, so it sits left of the word by the prompt's width.
- With the menu open, Esc pressed by hand and then a key more than 20 ms later counts as Esc, then that key.

## Changes

- unreleased: Ctrl+C pressed while a slow program is still answering aborts the line at once, instead of seeming to do nothing until the next key.
- unreleased: a line that gave up waiting for a slow program no longer makes the same line at a later prompt give up at once; `goh-unload` stops a pending wait.
- unreleased: the box lands under the word with `prompt_subst` prompts (oh-my-zsh, powerlevel10k, starship), and its right border stays straight with wide characters.
- unreleased: late values from slow programs appear without another key.
- unreleased: the inline menu is a floating framed box under the word being completed (`display.inlineStyle`, `box` by default; `flat` is the old look).
- unreleased: Esc closes the menu at once instead of after `KEYTIMEOUT` (0.4 s).
- unreleased: Enter on a picked suggestion runs the line (`display.enterRuns`, on); an option that wants its value or a folder is only inserted.
- unreleased: warnings on risky suggestions in bold red; `ssh` runs through `goh ssh` (see ssh-popup).
- 1.10.0: Enter accepts a lone suggestion.
- 1.4.0: inline menu with kind glyphs, warnings on every row and a detail line.
- 1.4.1, 1.5.0, 1.6.0: zsh-autosuggestions and plugin widget fixes.
- 1.0.0: first release.
