---
id: popup
title: Popup at the cursor
status: stable
since: 1.0.0
updated: 2026-11-06
area: display
platforms: [macos, linux]
settings: [popup.font, popup.fontSize, display.mode]
commands: [goh-popup --probe, goh-popup --resolve]
env: [GOH_POPUP_FONT]
---

# Popup at the cursor

`goh-popup` draws the menu in a small window anchored at the terminal's caret, in the terminal's colours and font.
Where the caret cannot be verified, the menu is drawn inline instead; nothing pretends to work.

## Behaviour

- macOS: the caret is read with the Accessibility API in iTerm2, Terminal.app, VS Code and the Claude app. The
  terminal needs the Accessibility permission (or goh-popup its own, see [popup-permission](popup-permission.md)).
- Linux: X11 and XWayland terminals that export `WINDOWID` and expose AT-SPI, with one visible terminal widget.
- The popup never takes focus or keys; the shell keeps selection. The wheel moves the highlight (macOS).
- Scrolled back so the line you are typing is out of view, the popup hides; it comes back when you scroll down.
- Font and size follow iTerm2 and VS Code settings, or `popup.font` / `popup.fontSize`.
- Each row shows its kind's Nerd Font icon (a command's logo in its brand's colour, a file's type), the footer the
  command's logo: the same icons as the inline menu's `display.icons: "nerd"`, drawn with a bundled font, so any
  `popup.font` shows them. File icons cover common source code, configuration, document, media, archive and font extensions; JSX and TSX use the React logo.
- A folder opened with → in zsh (see [zsh-adapter](zsh-adapter.md)) shows its entries under it with tree lines that
  run the full height of each row and join from row to row, the icon after them, as in the inline box; the footer
  then names ← and →.
- Over ssh it is drawn by the machine you connected from with [ssh-popup](ssh-popup.md); otherwise inline.

## Limits

- Inline in tmux, screen, native Wayland, unverified terminals: there the menu is a floating framed box drawn by the shell
  (see [zsh-adapter](zsh-adapter.md)).
- `goh-popup --probe` and `--resolve` explain the decision.

## Changes

- unreleased: the folder tree's lines are drawn as joined lines in front of each icon, and the footer names ←→ in a folder.
- unreleased: JSX and TSX suggestions use the React logo.
- unreleased: File icons cover more programming languages, configuration files, documents, images, audio, video, archives and fonts, including JSX, TSX and JavaScript/TypeScript module variants.
- unreleased: the popup hides while the terminal is scrolled back, instead of moving up over the lines being read.
- unreleased: the popup follows iTerm2's profile font again when iTerm2's preferences hold dates or data.
- unreleased: Nerd Font icons per kind, the same set as the inline menu, replace the command logo on every row.
- unreleased: where the popup cannot draw, zsh draws a floating framed box instead.
- unreleased: warnings on risky suggestions drawn in red.
- 1.8.0: Linux X11/AT-SPI caret and X11 surfaces.
- 1.7.1: VS Code's accessibility tree.
- 1.1.3: the Claude app's host fallback.
- 1.0.0: first release (macOS).
