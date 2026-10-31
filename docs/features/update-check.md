---
id: update-check
title: Update notice
status: stable
since: 1.5.0
updated: 2026-10-31
area: tooling
settings: [updates.check]
commands: [goh update]
env: [GOH_UPDATE_CHECK, GOH_UPDATE_STATE]
---

# Update notice

Opt in with `updates.check`: once a day goh reads the latest release number and prints one line when a shell
starts if a newer one exists. `goh update` checks once on request.

## Behaviour

- One `curl` request for GitHub's public release page; nothing about you or your commands is sent.

## Limits

- Off by default.

## Changes

- 1.5.0: first release.
