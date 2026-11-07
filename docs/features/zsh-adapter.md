---
id: zsh-adapter
title: zsh: menus as you type
status: stable
since: 1.0.0
updated: 2026-11-07
area: shells
shells: [zsh]
settings: [display.live, display.maxSuggestions, display.mode, display.enterRuns, display.inlineStyle, display.icons]
env: [GOH_LIVE, GOH_LIMIT, GOH_DISPLAY, GOH_ENTER_RUNS, GOH_INLINE_STYLE, GOH_ICONS]
---

# zsh: menus as you type

In zsh the menu opens and follows what you type, with ghost text for the best line; Tab, arrows and Enter work as
in an IDE. Load it with `goh setup` or `eval "$(goh init zsh)"`.

## Behaviour

- As you type (`display.live`, on): after each edit of a recognised command the menu opens or updates; it closes
  when nothing matches. Esc hides it until the line changes.
- Tab/→ insert the highlighted candidate; ↑↓ and Shift+Tab move, wrapping; Esc closes; Left closes and moves.
  Navigation keys are bound only while the menu is open.
- Folders open as a tree, for any command that takes paths (`ls`, `cd`, `cat`, `cp`, ...): → on a folder shows its
  entries under it with branch lines (`├─`, `└─`), one level deeper, and moves the highlight onto them; → on a
  folder inside goes another level down. ← goes back: it closes the folder you are in and puts the highlight on it.
  On a top-level row ← closes the menu as before. Tab or Enter on an entry puts its whole path in place of the word
  you typed (`ls pro` → `ls project/src/main.rs `). The line does not change while you browse. A folder holds up to
  50 entries, the same as the menu; only what is inside it goes under it (`cd`'s folders you visit often stay at the top). Rows above the window scroll out of view as you move down; the footer shows
  where you are (`7/23`). The popup draws the same tree.
- Esc closes at once, without zsh's `KEYTIMEOUT` wait: while the menu is open goh reads what follows Esc itself, and
  Alt+key, a paste or any other key starting with Esc closes the menu and does what it did before, including one bound
  to type text (`bindkey -s`, as oh-my-zsh's Alt+L).
- Enter takes the highlighted candidate after you moved in the menu, or the only one when there is just one, and runs
  the line; an option that wants its value, or a folder, is only inserted. `display.enterRuns: false` makes Enter
  only insert (a second Enter runs). With several untouched candidates Enter runs the line as typed. Ctrl+C ends
  the line and takes the inline menu off the screen before the next prompt; the line itself, with any ghost text, stays.
- Ghost text: the best line is shown after the cursor; → takes it.
- Where the popup cannot draw (Ghostty, Kitty, WezTerm, tmux, ssh, ...), the menu is a floating framed box under the word you
  are completing, in the terminal's own colours; no permission is needed. `display.inlineStyle: "flat"` draws plain
  rows instead, which a screen reader reads more easily. The box finds the word's column by measuring your prompt,
  including one built from `prompt_subst` parameters (powerlevel10k). A prompt that runs a command (starship, many
  oh-my-zsh themes) is not run again: at each prompt goh asks the terminal where the cursor is (`ESC[6n`) and takes
  the prompt's width from its answer. Row borders stay straight with wide characters
  (CJK, emoji), and the selected row and the border keep the box's background.
- The box's top border names what you are completing (`git push · options`), each row has its description beside the
  label, and the bottom border shows the position and keys. In a narrow terminal the descriptions go and a risky row
  keeps its `⚠`; the selected row's warning and description move to a line under the rows.
- `display.icons: "nerd"` draws Nerd Font icons: the command's logo in the title, an icon per kind and per file type.
  File icons cover common source code, configuration, document, media, archive and font extensions; JSX and TSX use the React logo.
  It needs a Nerd Font Mono as the terminal's font; `"unicode"`, the default, works with any font.
- A program still answering (a slow Docker, a cluster) shows `…` in the footer; its values appear when they arrive. The
  wait stops when you close the menu (Esc, Left) or move in it, and gives up after about 5 s; each new prompt starts
  the count again, so the same line typed later is asked again.
- WSL in a Windows Terminal pane: menus go to the Windows popup, `goh-popup.exe`, found on the Windows PATH that WSL
  adds to its own (`GOH_POPUP` names another). tmux, screen and ssh keep them inline, as elsewhere.
- Fail-open: any problem falls through to the widget Tab had before; `goh-unload` restores it.
- Works next to zsh-autosuggestions and plugins that replace `zle-line-init`.

## Limits

- Needs zsh; the menu is drawn by [popup](popup.md) where the caret is verified, else inline.
- A prompt that runs a command (`$(...)` or a backtick under `prompt_subst`), in a terminal that does not answer
  `ESC[6n` within half a second: the box anchors as if the prompt had no width, left of the word by the prompt's
  width, and goh stops asking for the rest of that shell.
- With the menu open, Esc pressed by hand and then a key more than 20 ms later counts as Esc, then that key.

## Changes

- unreleased: in WSL under Windows Terminal, menus are drawn by the Windows popup (`goh-popup.exe`).
- unreleased: an opened folder lists only its own entries; `cd` no longer puts folders you visit often elsewhere under it.
- unreleased: the box lands under the word with a prompt that runs a command (starship, oh-my-zsh themes): its width is read from the terminal instead of counting as none (#58).
- unreleased: → opens a folder as a tree under its row, and ← goes back out of it; Tab or Enter takes an entry's whole path.
- unreleased: a box of short labels widens enough to show its keys in the bottom border, which names ←→ inside a folder.

- unreleased: JSX and TSX suggestions use the React logo.
- unreleased: File icons cover more programming languages, configuration files, documents, images, audio, video, archives and fonts, including JSX, TSX and JavaScript/TypeScript module variants.
- unreleased: the box shows the command and kind in its top border, a description beside every label and the keys in its bottom border; a warning keeps its `⚠` in a narrow terminal, and wide labels no longer push the box past the edge. The ①②③ marks on suggestions picked before are gone; those suggestions still rank first.
- unreleased: `display.icons: "nerd"` draws Nerd Font icons and command logos in the inline menu.
- unreleased: Ctrl+C with the inline menu open no longer leaves the menu on the screen above the next prompt.
- unreleased: Esc closes the menu at once next to a key bound to type text that starts with Esc (`bindkey -s`, as oh-my-zsh's Alt+L), instead of after `KEYTIMEOUT` (0.4 s); that key types its text again once the menu closes.
- unreleased: Ctrl+C pressed while a slow program is still answering aborts the line at once, instead of seeming to do nothing until the next key.
- unreleased: a line that gave up waiting for a slow program no longer makes the same line at a later prompt give up at once; `goh-unload` stops a pending wait.
- unreleased: the selected row's marker, glyph and label, and the box's border, keep the box's background (zsh 5.9 showed the terminal's background through them).
- unreleased: the box lands under the word with prompts built from `prompt_subst` parameters (powerlevel10k); a prompt that runs a command (starship, oh-my-zsh) no longer throws it to a far column: it anchors as if the prompt had no width. Its right border stays straight with wide characters.
- unreleased: late values from slow programs appear without another key.
- unreleased: the inline menu is a floating framed box under the word being completed (`display.inlineStyle`, `box` by default; `flat` is the old look).
- unreleased: Esc closes the menu at once instead of after `KEYTIMEOUT` (0.4 s).
- unreleased: Enter on a picked suggestion runs the line (`display.enterRuns`, on); an option that wants its value or a folder is only inserted.
- unreleased: warnings on risky suggestions in bold red; `ssh` runs through `goh ssh` (see ssh-popup).
- 1.10.0: Enter accepts a lone suggestion.
- 1.4.0: inline menu with kind glyphs, warnings on every row and a detail line.
- 1.4.1, 1.5.0, 1.6.0: zsh-autosuggestions and plugin widget fixes.
- 1.0.0: first release.
