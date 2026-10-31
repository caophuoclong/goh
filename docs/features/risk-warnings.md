---
id: risk-warnings
title: Warnings on risky suggestions
status: stable
since: 1.4.0
updated: 2026-10-31
area: core
---

# Warnings on risky suggestions

Suggestions that can do damage carry a warning in their description, and say which cluster or workspace they act
on. Nothing is blocked or run.

## Behaviour

- git (`push --force`, `reset --hard`, `clean -f`, `branch -D`, `stash drop|clear`), `rm -r|-f`, docker prune,
  kubectl/helm changes (with the kubectl context), terraform apply/destroy (with the workspace).
- Shown on every row of the inline menu and in the popup.

## Limits

- Only options and subcommands listed in `crates/goh-daemon/src/risk.rs`.

## Changes

- 1.4.0: warnings on every row of the inline menu.
