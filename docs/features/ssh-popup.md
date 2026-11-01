---
id: ssh-popup
title: Popup over ssh
status: experimental
since: unreleased
updated: 2026-11-01
area: ssh
shells: [zsh]
platforms: [macos, linux]
settings: [popup.overSsh]
commands: [goh ssh, goh popup-bridge]
env: [LC_GOH_POPUP, GOH_POPUP_SSH]
---

# Popup over ssh

Over ssh, the remote zsh's menus are drawn by the popup on the machine you connected from, at the caret of the
terminal you typed `ssh` in. In a shell with the zsh adapter, `ssh` runs through `goh ssh` by itself.

## Behaviour

- `goh ssh ARGS` (or plain `ssh`, wrapped by the zsh adapter) forwards a private Unix socket to
  `/tmp/goh-popup-<random>.sock` on the host (`-R`, mode 0600) and names it in `LC_GOH_POPUP`.
- The host's adapter sends its frames through `goh popup-bridge $LC_GOH_POPUP`; keys and edits stay in the remote
  shell. Each connection gets its own local `goh-popup`.
- iTerm2's cell height is asked locally before ssh starts and added to frames that lack it.
- Without the relay the bridge answers inline. `goh doctor` on the host says which applies.
- The wrapper is not defined over ssh, in tmux/screen, or over your own `ssh` alias or function. It is defined with
  `display.mode: inline` too: menus here stay inline, a host's are drawn by the popup. `popup.overSsh: false` turns
  it off.
- `ssh host command`, `-N`, `-f`, `-W`, `-O`, `-G`, `-V` are plain `exec ssh`.

## Limits

- The host's sshd must accept `LC_*` variables (Debian and Ubuntu do).
- Inline inside tmux or screen on the host. zsh hosts only.
- sshd does not remove the forwarded socket file when the session ends; `/tmp` is cleared later.

## Changes

- unreleased: `ssh` runs through `goh ssh` with `display.mode: inline` too; only `popup.overSsh: false` turns it off.
- unreleased: first version (`goh ssh`, `goh popup-bridge`, `popup.overSsh`).
