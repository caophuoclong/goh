---
id: completion-engine
title: Spec-driven completion
status: stable
since: 1.0.0
updated: 2026-10-31
area: core
commands: [goh menu, goh complete]
---

# Spec-driven completion

Suggests subcommands, options, arguments and paths for the line being typed, with a description for each, from
declarative JSON completion specs. Nothing is executed for you: only the candidate you pick is inserted.

## Behaviour

- The line is parsed against the command's spec; candidates are the subcommands, options and arguments that fit
  where the cursor is, ranked by match and use. Options already given are skipped; `ls -la` reads as `-l -a`.
- Wrappers are seen through: after `sudo`, `time`, `env`, `xargs`, `watch`, `nohup`, `npx` and `FOO=1` the next
  command is completed by its own spec. Shell aliases (`git co` from `[alias]` too) are expanded first.
- A slip is corrected (`git chekcout` finds `checkout`); files are narrowed by type (`python <TAB>` lists `.py`).
- `-` is offered for the previous branch or directory; `$VARIABLES` complete from the environment.
- Directories rank by their last component.

## Limits

- A command with no spec gets no suggestions unless [bundled-specs](bundled-specs.md) auto specs are on.
- Offsets on the wire are UTF-8 bytes; adapters convert.

## Changes

- 1.10.1: directories rank by their last component.
- 1.7.0: wrappers, git aliases.
- 1.1.2: an alias keeps its own spec when its expansion has none.
- 1.0.0: first release.
