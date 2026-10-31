# fish adapter

Completion for commands that have a goh spec, through fish's own `complete`: key bindings, pager and
ghost text stay fish's. Source `goh.fish` from `config.fish` (or copy it to `conf.d/`) with `goh` on `PATH`.

`cd` also suggests frequently used directories once a word is typed: zoxide when it is installed and has
entries, otherwise goh's own history (`~/.local/share/goh/dirs`), which the adapter fills on every directory change.

Checked in CI by crates/goh-daemon/tests/fish_adapter.rs.
