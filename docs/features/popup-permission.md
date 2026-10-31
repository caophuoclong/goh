---
id: popup-permission
title: goh-popup's own Accessibility permission
status: experimental
since: 1.6.0
updated: 2026-10-31
area: display
platforms: [macos]
settings: [popup.ownPermission]
env: [GOH_POPUP_OWN_PERMISSION, GOH_MACOS_SIGN_IDENTITY, GOH_MACOS_SIGN_P12]
---

# goh-popup's own Accessibility permission

On macOS, `popup.ownPermission` asks for the Accessibility permission once, for goh-popup itself, instead of once
per terminal app. Signed builds keep it across updates and rebuilds.

## Behaviour

- The popup runs as its own responsible process; at its first menu macOS asks for **goh-popup**.
- Release packages ship `goh-popup.app`, signed with goh's own certificate, so the permission stays through updates.
- Local builds are signed with the same certificate when it is found: `scripts/macos-sign.sh` uses
  `GOH_MACOS_SIGN_IDENTITY` (release workflow) or `../macos-sign/goh-signing.p12` / `GOH_MACOS_SIGN_P12`, imported
  into a keychain that lives only while it runs. `install.sh --from-source` and `scripts/package.sh` install or
  pack the signed bundle; after `cargo build`, `scripts/macos-sign.sh target/debug/goh-popup`.
- Without a certificate the build is signed ad hoc and macOS asks again after each rebuild.

## Limits

- macOS only. `goh doctor` still reports the terminal's own permission.

## Changes

- unreleased: goh-popup checks its permission again for about a second before asking, so a just-started popup does not prompt when it is allowed.
- unreleased: local builds signed with goh's certificate (`scripts/macos-sign.sh`, `scripts/macos-popup-app.sh`).
- 1.6.0: first release.
