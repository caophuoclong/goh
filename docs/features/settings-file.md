---
id: settings-file
title: Settings file
status: stable
since: 1.0.0
updated: 2026-11-07
area: tooling
commands: [goh config, goh config show, goh config path, goh config env]
env: [GOH_CONFIG]
files: [~/.config/goh/config.json]
---

# Settings file

`goh config` opens `~/.config/goh/config.json` (or `$GOH_CONFIG`) in `$EDITOR`, creating it on first use, and checks
it when the editor closes. `goh config show` prints and checks it, `goh config path` says where it is.

## Behaviour

- A new file lists every setting with its default and one short comment.
- Comments (`//`, `/* */`) are allowed. An environment variable you set yourself (`GOH_LIVE=off`, ...) wins over the
  file. Open shells pick up a change after `exec $SHELL`; the daemon needs nothing.
- A file goh wrote earlier (with no comments, or with the older, longer ones) is rewritten once in the current layout
  when `goh config` opens it. Its settings stay; `configVersion` and the unused `privacy.telemetry`,
  `networkSpecs`, `dynamicGenerators` and `retainCommandHistory` are dropped. The old file is kept next to it as
  `config.json.bak` (or `.bak.2`, ... when that exists).

## Limits

- A file with comments of your own is never rewritten.

## Changes

- unreleased: shorter template, one comment line per setting; the unused `configVersion` and reserved privacy keys
  are no longer written, and files in the old layout are rewritten once.
