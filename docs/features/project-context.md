---
id: project-context
title: Project-aware suggestions
status: stable
since: 1.2.0
updated: 2026-10-31
area: core
---

# Project-aware suggestions

Arguments come from the project you are in and the tools you use: branches, scripts, targets, services, packages.

## Behaviour

- git: branches, tags, stashes, recent commits, changed files, your `[alias]`es.
- `package.json` scripts and `node_modules/.bin`; `pyproject.toml`/`Pipfile` scripts; `deno.json` tasks.
- Makefile, justfile and Taskfile targets; compose services; Cargo bins, examples, features and workspace packages.
- `$CDPATH` folders; kubectl pods and namespaces (asked in the background).
- Package names for brew, npm, pnpm, yarn, bun, cargo, pip and uv.
- ssh, scp, rsync hosts from `~/.ssh/config`, known hosts and history.

## Limits

- Read from files on the machine the shell runs on; over ssh that is the host, so it needs goh there
  ([ssh-host-copy](ssh-host-copy.md)).

## Changes

- 1.7.0: kubectl, project scripts.
- 1.2.0: package names.
