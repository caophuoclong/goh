# bash adapter (spike)

Completion through bash's programmable completion; no key bindings are touched (see docs/docs/18-spike-results.md).

```bash
cargo build && export PATH="$PWD/target/debug:$PATH"
mkdir -p ~/.config/goh/specs && cp docs/examples/git.completion.json ~/.config/goh/specs/git.json
source adapters/bash/goh.bash      # after bash-completion; `goh-unload` removes it
```

Only commands that have a spec and no existing completion are registered. Tab, the candidate list and filename
fallback stay bash's own. Verified on bash 3.2 (macOS) and bash 5.2 (Debian, Linux arm64, in Docker); real Linux terminal emulators and bash 4.x are not.
