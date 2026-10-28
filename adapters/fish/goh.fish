# goh.fish — spec-driven completion for fish through its own `complete` (docs/05).
#
# Like the bash adapter, this does not touch key bindings: fish keeps its pager, ghost text and fuzzy
# matching, and goh only adds candidates for commands that have a spec. Anything unusual yields no
# candidates (fail-open). Source it from ~/.config/fish/config.fish, or copy it to
# ~/.config/fish/conf.d/goh.fish. `goh-unload` removes the directory-change hook; fish cannot remove
# only our completions, so restart the shell to drop those.
#
# `cd` also suggests frequently used directories (zoxide when installed, otherwise goh's own history,
# which this file feeds on every directory change).

set -q GOH_CTL; or set -g GOH_CTL goh
set -g _goh_session fish-$fish_pid-(random)
# Lets `goh doctor` tell whether this very shell loaded the adapter (it compares the pid with its parent's).
set -gx GOH_ADAPTER fish:$fish_pid

function __goh_complete
    set -l buffer (commandline -cp)
    set -l cur (commandline -ct)
    test -n "$buffer"; or return 0
    set -l point (string length --bytes -- $buffer)
    set -l cur_bytes (string length --bytes -- $cur)
    set -l sep (printf '\x1f')
    command $GOH_CTL menu --session $_goh_session --cwd $PWD --limit 50 \
        --cursor-byte $point --buffer $buffer 2>/dev/null | while read -l line
        set -l f (string split -- $sep $line)
        # Only candidates that replace exactly the word being completed.
        test "$f[1]" = (math $point - $cur_bytes); and test "$f[2]" = "$point"; or continue
        set -l insert (string unescape -- (string trim --right --chars=' ' -- $f[6]))
        printf '%s\t%s\n' $insert $f[7]
    end
end

function __goh_visit --on-variable PWD
    command $GOH_CTL visit $PWD >/dev/null 2>&1 &
end

function goh-load
    command $GOH_CTL specs 2>/dev/null | while read -l -d \t name source subcommands replaces
        test -n "$name"; or continue
        complete -c $name -a '(__goh_complete)'
    end
    __goh_visit
end

function goh-unload
    functions -e __goh_visit
end

goh-load
