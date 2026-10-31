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
- `$CDPATH` folders; kubectl pods and namespaces; Docker containers, images, networks and volumes; tmux sessions
  and windows (`tmux attach-session -t`); systemd units (`systemctl restart`); running processes for `kill` (by pid)
  and `pkill`/`killall` (by name).
- Programs asked for values (docker, git, zoxide) get 120 ms per keystroke; a slower one finishes in the background
  (up to 5 s) and its answer shows from the next keystroke, so a slow Docker host still lists its containers.
  kubectl is never waited for.
- Package names for brew, npm, pnpm, yarn, bun, cargo, pip and uv.
- ssh, scp, rsync hosts from `~/.ssh/config`, known hosts and history.

## Limits

- Read from files on the machine the shell runs on; over ssh that is the host, so it needs goh there
  ([ssh-host-copy](ssh-host-copy.md)).

## Changes

- unreleased: Docker networks and volumes, tmux sessions and windows, systemd units and running processes are suggested.
- unreleased: a Docker (or git, zoxide) slower than 120 ms is asked in the background, so `docker exec` lists the
  containers from the next keystroke instead of never.
- 1.7.0: kubectl, project scripts.
- 1.2.0: package names.
