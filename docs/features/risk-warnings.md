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

- A risky suggestion's description starts with `⚠ what it does`, then `· context: <kubectl context>` or
  `· workspace: <terraform workspace>` where that applies, then its own description.
- The warning is drawn in red on every row, not only the selected one, so it is seen before it is picked: bold red
  in the inline menu (bold only with `NO_COLOR`), red in the popup (a light or dark red to suit the surface).
- Listed per command (`crates/goh-daemon/src/risk.rs`):
  - git: `push --force|--force-with-lease|--mirror|--prune|--delete`, `reset --hard`, `clean -f`, `branch -D`,
    `checkout -f`, `tag -d`, `stash drop|clear`, `reflog expire`, `gc --prune`, `filter-branch`;
  - databases: `prisma migrate reset|deploy`, `prisma db push --force-reset|--accept-data-loss`,
    `prisma db execute`, `supabase db reset|push`, `rake db:drop|db:reset|db:schema:load`, `psql -c|--command`;
  - containers and clusters: docker `rm`, `rmi`, `container|image|volume rm`, the `prune`s (`--volumes`, `-a`),
    `compose down` (`-v`); kubectl and helm changes; terraform `apply`, `destroy`, `state rm`,
    `workspace delete`, `-auto-approve`;
  - files and machines: `rm -r|-f`, `find -delete`, `rsync --delete*`, `rclone sync|purge`, `tar --remove-files`,
    `chmod|chown -R`, `mv|cp -f`, `systemctl stop|disable|mask|poweroff|reboot`, `apt purge|autoremove`,
    `gpg --delete-keys|--delete-secret-keys`;
  - what others see: `npm|pnpm|yarn|bun publish`, `npm unpublish`, `cargo publish`, `gh repo|release delete`,
    `aws s3 rm|rb|sync --delete`, `serverless remove`.
- Any other command's subcommand named `delete`, `destroy`, `drop`, `purge`, `wipe`, `erase`, `remove`, `rm`,
  `uninstall`, `prune`, `flush`, `flushall`, `flushdb` or `truncate` is flagged by its name (`minikube delete`,
  `gcloud ... delete`); git's are not, its list above is exact.

## Limits

- Only suggestions are flagged: what is already typed, and SQL inside a quoted argument (`psql -c "DROP TABLE"`),
  is not read.
- A command with no spec gets no suggestions, so no warning.

## Changes

- unreleased: databases, docker, cloud, publishing and system commands, any destructive-named subcommand; drawn in red inline and in the popup.
- 1.4.0: warnings on every row of the inline menu.
