---
id: theme
title: Colours from the terminal
status: stable
since: 1.0.0
updated: 2026-10-31
area: display
settings: [theme.mode]
commands: [goh theme]
---

# Colours from the terminal

The menu inherits the terminal's colours instead of imposing a theme.

## Behaviour

- The terminal is asked for its default colours (OSC 10/11) between prompts; `COLORFGBG` and the config are
  fallbacks. `goh theme` shows the result, `goh-theme-refresh` asks again.

## Limits

- Not asked inside tmux or screen.

## Changes

- 1.0.0: first release.
