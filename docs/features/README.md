# Feature specs

One file per feature: what it does for the user, its settings, commands and limits, and a log of its changes. The
release notes, the README and any other docs are written from these files, so they must stay true to the code.

**Every change a user can notice updates the spec of the feature it touches, in the same pull request**: the text,
`updated`, and a line under `## Changes` (`- unreleased: ...`; the release turns it into the version). A new feature
gets a new file and a row in the index below. CI checks the format of every spec, and fails a `feat`, `fix` or
`perf` pull request that changes what goh does (`crates/*/src`, the bundled specs, `adapters/`, `install.sh`,
`packaging/`) without touching a spec here; the `no-spec` label skips that for a change no user can notice.
`scripts/check-feature-specs.sh` runs the same checks locally.

## Format

```markdown
---
id: ssh-popup                  # the file name, without .md
title: Popup over ssh
status: experimental           # stable | experimental | planned | removed
since: unreleased              # the first release with it, or `unreleased`
updated: 2026-10-31            # the last change to this file
area: ssh                      # core | shells | display | ssh | tooling
shells: [zsh]                  # optional, as are the next ones
platforms: [macos, linux]
settings: [popup.overSsh]      # keys in `goh config`
commands: [goh ssh]
env: [LC_GOH_POPUP]
files: [~/.local/share/goh/ssh-hosts]
---

# Popup over ssh

What it gives the user, in a few lines.

## Behaviour
## Limits
## Changes

- unreleased: what changed.
- 1.6.0: earlier changes, newest first.
```

Write for a user: what happens, what to type, what it needs. Implementation notes belong in the code.

## Index

| Spec | Area | Status | Since |
|---|---|---|---|
| [completion-engine](completion-engine.md) | core | stable | 1.0.0 |
| [bundled-specs](bundled-specs.md) | core | stable | 1.0.0 |
| [project-context](project-context.md) | core | stable | 1.2.0 |
| [history-suggestions](history-suggestions.md) | core | experimental | 1.6.0 |
| [risk-warnings](risk-warnings.md) | core | stable | 1.4.0 |
| [snippets](snippets.md) | core | stable | 1.0.0 |
| [zsh-adapter](zsh-adapter.md) | shells | stable | 1.0.0 |
| [bash-adapter](bash-adapter.md) | shells | stable | 1.0.0 |
| [other-shells](other-shells.md) | shells | experimental | 1.0.0 |
| [powershell-adapter](powershell-adapter.md) | shells | experimental | unreleased |
| [popup](popup.md) | display | stable | 1.0.0 |
| [popup-permission](popup-permission.md) | display | experimental | 1.6.0 |
| [theme](theme.md) | display | stable | 1.0.0 |
| [ssh-popup](ssh-popup.md) | ssh | experimental | unreleased |
| [ssh-host-copy](ssh-host-copy.md) | ssh | experimental | unreleased |
| [doctor](doctor.md) | tooling | stable | 1.0.0 |
| [install](install.md) | tooling | stable | 1.0.0 |
| [update-check](update-check.md) | tooling | stable | 1.5.0 |
