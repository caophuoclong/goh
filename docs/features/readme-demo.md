---
id: readme-demo
title: Reproducible README demo
status: stable
since: unreleased
updated: 2026-11-06
area: tooling
---

# Reproducible README demo

The README shows two short looping recordings of goh running in zsh. Contributors can regenerate them from the
current checkout with `scripts/record-demo.sh`; setup and dependencies are in `docs/media/README.md`.

## Behaviour

- Records real live suggestions, arrow-key navigation, Tab insertion, project branches, Docker flags,
  force-push warnings, typo correction and expandable folders through the inline menu.
- A second recording shows npm scripts, Makefile targets, snippet expansion and opt-in history suggestions
  with a placeholder secret masked by goh.
- Builds the application and creates a temporary Git repo, home, shell config and private daemon. Personal
  history, config and completion specs do not affect the recording; the fixture is removed afterwards.
- Publishes `docs/media/features.gif` and `docs/media/demo.gif` after checking
  visible completion text, a real branch switch, 1100 px width, 10–20 seconds and a 3 MB maximum size.
- Each recording returns to its initial clean prompt for the loop. The README embeds each animation once.

## Limits

- Requires Rust, Git, zsh, VHS, ttyd, FFmpeg and the tape's font (Menlo on macOS).
- Shows the inline menu. Native overlay windows need real desktop screen capture.
- The warning demonstrates a risky suggestion, not a command-blocking safety mechanism. The line is never run.
- Timing and rendering can vary with the host, font and VHS version. Inspect extracted frames and the GIF after
  regenerating; the script rejects recordings outside its duration or size limits.

## Changes

- unreleased: add an isolated, automated and verified README recording with live suggestions, branch selection,
  Docker flags, force-push warnings, typo correction and regeneration instructions.
