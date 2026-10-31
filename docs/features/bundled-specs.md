---
id: bundled-specs
title: Bundled specs and specs from --help
status: stable
since: 1.0.0
updated: 2026-10-31
area: core
settings: [completion.autoSpecs]
commands: [goh specs, goh spec gen, goh reload]
files: [~/.config/goh/specs/]
---

# Bundled specs and specs from --help

goh ships completion specs for 191 common commands and can write one for any other program from its `--help`.

## Behaviour

- Bundled: `git`, `docker`, `npm`, `pnpm`, `cargo`, `kubectl`, `just`, `task`, `brew`, `aws`, `deno`, `gradle`,
  `flutter`, `jq`, `tmux`, `systemctl` and more (`goh specs` lists them).
- Your own specs go in `~/.config/goh/specs/` and replace a bundled one of the same name; `goh reload` reads them.
- `goh spec gen <command>` writes a spec read from the command's `--help`.
- With `completion.autoSpecs` on, the first time you type a program with no spec, `<program> --help` runs once
  in the background and its spec is saved.
- `goh specs --missing` lists programs you ran that have none (needs `privacy.recordCommands`).

## Limits

- Auto specs are off by default: they run a program's `--help`.

## Changes

- unreleased: 77 more specs (kill, lsof, pytest, ruff, podman, pulumi, cmake, adb, nx, bat, fd, ...).
- 1.7.0: 18 more specs.
- 1.3.0: 39 more specs; `goh specs --missing`.
- 1.0.0: first release.
