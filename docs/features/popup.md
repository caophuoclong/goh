---
id: popup
title: Popup at the cursor
status: stable
since: 1.0.0
updated: 2026-10-31
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
- Font and size follow iTerm2 and VS Code settings, or `popup.font` / `popup.fontSize`.
- Over ssh it is drawn by the machine you connected from with [ssh-popup](ssh-popup.md); otherwise inline.

## Limits

- Inline in tmux, screen, native Wayland, unverified terminals.
- `goh-popup --probe` and `--resolve` explain the decision.

## Changes

- 1.8.0: Linux X11/AT-SPI caret and X11 surfaces.
- 1.7.1: VS Code's accessibility tree.
- 1.1.3: the Claude app's host fallback.
- 1.0.0: first release (macOS).
