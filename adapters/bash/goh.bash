# goh.bash — spec-driven completion for bash through programmable completion (docs/05, SPIKE-02).
#
# bash has no safe per-keystroke hook: `bind -x` handlers cannot call back into readline, so they
# cannot fall through to native completion, and READLINE_LINE only exists from bash 4. This adapter
# therefore does not touch any key binding. It registers a completion function for commands that
# have a spec; Tab, the candidate list and every other binding stay bash's own. Anything unusual
# returns no candidates, and `-o default` hands the word back to filename completion (fail-open).
#
# Source it from ~/.bashrc after bash-completion. Commands that already have a completion spec
# (e.g. git from bash-completion) are left alone. `goh-unload` removes what it registered.
#
# Limits: only commands with a spec (not the command word); candidates must extend the word being
# completed (no fuzzy matching, which readline cannot display); with non-ASCII text and the cursor
# not at the end of the line it offers nothing, because COMP_POINT counts bytes on bash 3.2 and
# characters on bash 5.2 (measured; other versions unknown).

_GOH_CTL=${GOH_CTL:-goh-ctl}
_GOH_SESSION="bash-$$-$RANDOM"
_GOH_REGISTERED=()

_goh_complete() {
  COMPREPLY=()
  local cur=${COMP_WORDS[COMP_CWORD]}
  local chars=${#COMP_LINE} cur_chars=${#cur}

  # Byte lengths (the wire uses UTF-8 byte offsets).
  local LC_ALL=C
  local bytes=${#COMP_LINE} cur_bytes=${#cur} point=$COMP_POINT
  if (( bytes != chars )); then
    # Non-ASCII: only the unambiguous "cursor at the end" case is safe.
    (( point == chars || point == bytes )) || return 0
    point=$bytes
  fi
  (( point == bytes )) || [[ $COMP_LINE != *[![:print:]]* ]] || return 0

  local out line start end cursor kind label insert desc ranges
  out=$(command "$_GOH_CTL" menu --session "$_GOH_SESSION" --cwd "$PWD" --limit 50 \
        --cursor-byte "$point" --buffer "$COMP_LINE" 2>/dev/null) || return 0

  local IFS=$'\n'
  for line in $out; do
    IFS=$'\x1f' read -r start end cursor kind label insert desc ranges <<<"$line"
    # Only candidates that replace exactly the word being completed and extend what is typed.
    [[ $start == $(( point - cur_bytes )) && $end == "$point" ]] || continue
    [[ $insert == "$cur"* ]] || continue
    COMPREPLY+=("$insert")
  done
}

goh-load() {
  local dir=${GOH_SPEC_DIR:-$HOME/.config/goh/specs} f name
  for f in "$dir"/*.json; do
    [[ -e $f ]] || continue
    name=${f##*/}; name=${name%.json}
    complete -p "$name" >/dev/null 2>&1 && continue          # someone else's completion wins
    complete -o nospace -o default -o bashdefault -F _goh_complete "$name" 2>/dev/null \
      || complete -o nospace -o default -F _goh_complete "$name" 2>/dev/null \
      || continue
    _GOH_REGISTERED+=("$name")
  done
}

goh-unload() {
  local name
  for name in "${_GOH_REGISTERED[@]}"; do complete -r "$name" 2>/dev/null; done
  _GOH_REGISTERED=()
}

goh-load
