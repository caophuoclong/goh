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

_GOH_CTL=${GOH_CTL:-goh}
_GOH_SESSION="bash-$$-$RANDOM"
# Lets `goh doctor` tell whether this very shell loaded the adapter (it compares the pid with its parent's).
export GOH_ADAPTER="bash:$$"
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

# Command record (privacy.recordCommands, GOH_RECORD=on; off by default): a successful line is handed to
# `goh record`, which leaves out anything that looks secret. bash has no preexec, so the line is read back from
# history after it ran; a line starting with a space is not in history (HISTCONTROL=ignorespace) and not recorded.
_goh_record() {
  local st=$?
  local line; line=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null | sed 's/^ *[0-9]* *//')
  # Only a new history entry is a command that ran (an empty Enter adds none).
  if (( st == 0 )) && [[ -n $line && ${_GOH_LAST_NUM-} != "$HISTCMD" ]]; then
    (command "$_GOH_CTL" record --cwd "$PWD" -- "$line" >/dev/null 2>&1 &)
  fi
  _GOH_LAST_NUM=$HISTCMD
  return $st
}

# Directory history for `cd` suggestions when zoxide is not installed (read by gohd; written here).
_goh_visit() {
  [[ $PWD == "${_GOH_LAST_PWD-}" ]] && return 0
  _GOH_LAST_PWD=$PWD
  (command "$_GOH_CTL" visit "$PWD" >/dev/null 2>&1 &)
}

goh-load() {
  local record=0 cfg
  cfg=$(command "$_GOH_CTL" config env 2>/dev/null)
  [[ $cfg == *GOH_RECORD=on* ]] && record=1
  # `updates.check`: one line, when the shell starts, if a newer release is known (see `goh update`).
  [[ $cfg == *GOH_UPDATE_CHECK=on* ]] && command "$_GOH_CTL" update --notice 2>/dev/null
  _GOH_LAST_NUM=$HISTCMD   # what was in history when the shell started is not a command to record
  if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
    # `_goh_record` goes first: it needs the status of the command that just ended.
    (( record )) && PROMPT_COMMAND=(_goh_record "${PROMPT_COMMAND[@]}")
    PROMPT_COMMAND+=(_goh_visit)
  else
    local first=_goh_visit
    (( record )) && first="_goh_record;_goh_visit"
    PROMPT_COMMAND="$first${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  fi
  local dir=${GOH_SPEC_DIR:-$HOME/.config/goh/specs} name source subcommands replaces
  # Use the daemon's loader catalog, including bundled commands and user overrides.
  while IFS=$'\t' read -r name source subcommands replaces; do
    [[ -n $name ]] || continue
    complete -p "$name" >/dev/null 2>&1 && continue          # someone else's completion wins
    complete -o nospace -o default -o bashdefault -F _goh_complete "$name" 2>/dev/null \
      || complete -o nospace -o default -F _goh_complete "$name" 2>/dev/null \
      || continue
    _GOH_REGISTERED+=("$name")
  done < <(GOH_SPEC_DIR="$dir" command "$_GOH_CTL" specs 2>/dev/null)
}

goh-unload() {
  local name
  if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
    PROMPT_COMMAND=("${PROMPT_COMMAND[@]/_goh_visit}")
    PROMPT_COMMAND=("${PROMPT_COMMAND[@]/_goh_record}")
  else
    PROMPT_COMMAND=${PROMPT_COMMAND/_goh_record;}; PROMPT_COMMAND=${PROMPT_COMMAND/_goh_record}
    PROMPT_COMMAND=${PROMPT_COMMAND/_goh_visit;}; PROMPT_COMMAND=${PROMPT_COMMAND/_goh_visit}
  fi
  for name in "${_GOH_REGISTERED[@]}"; do complete -r "$name" 2>/dev/null; done
  _GOH_REGISTERED=()
}

goh-load
