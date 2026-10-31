# goh

- Every change a user can notice updates its feature's spec in `docs/features/` in the same change: the text,
  `updated`, and a `- unreleased: ...` line under `## Changes`. A new feature gets a new spec and a row in
  `docs/features/README.md`. Run `scripts/check-feature-specs.sh` before committing.
- Commit messages and PR titles follow Conventional Commits (see CONTRIBUTING.md); PRs are squash-merged.
