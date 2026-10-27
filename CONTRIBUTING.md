# Contributing

## Commit messages decide the version

goh uses [Semantic Versioning](https://semver.org) (`MAJOR.MINOR.PATCH`, tagged `vX.Y.Z`) and
[Conventional Commits](https://www.conventionalcommits.org). The next version is worked out from the commits since
the last release; nobody edits a version number by hand.

| Commit (or squash-merged PR title) | Next version | In the changelog |
|---|---|---|
| `fix: stop the menu closing on Esc` | patch, 1.0.0 → 1.0.1 | Bug fixes |
| `perf: cache git branches` | patch | Performance |
| `feat: complete just recipes` | minor, 1.0.1 → 1.1.0 | Features |
| `feat!: rename privacy.learnChoices`, or a `BREAKING CHANGE:` footer | major, 1.1.0 → 2.0.0 | Features, marked breaking |
| `docs:` `chore:` `refactor:` `test:` `ci:` `build:` | no release | hidden |

A scope is optional: `fix(zsh): ...`, `feat(spec): ...`.

**What is a breaking change** (the public surface): `goh` commands and flags; the settings file keys and
`configVersion`; the completion spec JSON (`schemaVersion`) and the format of `snippets.json`; the environment
variables the adapters read (`GOH_*`); behaviour of the keys the adapters bind. Internals (the socket protocol between
a `goh` and a `gohd` of the same version, the crates' Rust APIs) are not.

Pull requests are squash-merged, so **the PR title is the commit message**; a check fails the PR when it does not
follow the format.

## Before you push

```bash
cargo test --workspace --locked      # the whole suite, including real zsh/bash on a pseudo-terminal
shellcheck install.sh scripts/*.sh
```
