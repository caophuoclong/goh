# bash adapter (spike)

Completion through bash's programmable completion; no key bindings are touched (see docs/docs/18-spike-results.md).

```bash
cargo build && export PATH="$PWD/target/debug:$PATH"
source adapters/bash/goh.bash      # after bash-completion; `goh-unload` removes it
```

Only commands that have a spec and no existing completion are registered. Tab, the candidate list and filename
fallback stay bash's own. Verified on bash 3.2 (macOS) and bash 5.2 (Debian, Linux arm64, in Docker); real Linux terminal emulators and bash 4.x are not.

The 40 bundled commands, including `git`, `docker`, `npm`, `pnpm`, `brew` and `cargo`, are registered from the same
catalog as the daemon. Your own specs go in `~/.config/goh/specs/` (see adapters/zsh/README.md).

With `privacy.recordCommands` on (`goh config`), each successful command is also handed to `goh record` from
`PROMPT_COMMAND` (bash has no pre-exec hook, so the line is read back from history; lines starting with a space are not
in history and not recorded). That gives `kubectl -n <TAB>` the values you used before. Whole-line history
suggestions and the next-command prediction need a line editor hook that bash does not offer, so they are zsh-only.
