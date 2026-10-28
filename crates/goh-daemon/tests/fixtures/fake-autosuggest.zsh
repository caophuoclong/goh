# Stands in for zsh-autosuggestions in adapter tests, with the parts goh meets: after each typed key it asks for a
# suggestion unless disabled (one request at a time, in the plugin's own fd variable: a new one cancels the last),
# and the answer arrives later (zle -F, as in the plugin's async mode) and goes into POSTDISPLAY whatever is there;
# autosuggest-accept appends POSTDISPLAY to the line, as the plugin's does.
typeset -g _ZSH_AUTOSUGGEST_ASYNC_FD=
_fake_as_answer() {
  local s
  IFS= read -r -u $1 s
  zle -F $1
  exec {1}<&-
  _ZSH_AUTOSUGGEST_ASYNC_FD=
  zle autosuggest-suggest -- "$s"
}
_fake_as_self_insert() {
  zle .self-insert
  if [[ -n $_ZSH_AUTOSUGGEST_ASYNC_FD ]]; then zle -F $_ZSH_AUTOSUGGEST_ASYNC_FD; exec {_ZSH_AUTOSUGGEST_ASYNC_FD}<&-; _ZSH_AUTOSUGGEST_ASYNC_FD=; fi
  (( ${+_ZSH_AUTOSUGGEST_DISABLED} )) && return
  exec {_ZSH_AUTOSUGGEST_ASYNC_FD}< <(sleep 0.2; print -r -- "${BUFFER}-from-history")
  zle -F $_ZSH_AUTOSUGGEST_ASYNC_FD _fake_as_answer
}
_fake_as_suggest() { POSTDISPLAY=${1#$BUFFER}; zle -R }   # the plugin's widgets end with a redisplay
_fake_as_disable() { typeset -g _ZSH_AUTOSUGGEST_DISABLED; POSTDISPLAY= }
_fake_as_enable() { unset _ZSH_AUTOSUGGEST_DISABLED }
_fake_as_accept() { BUFFER+=$POSTDISPLAY; POSTDISPLAY=; CURSOR=$#BUFFER }
zle -N self-insert _fake_as_self_insert
zle -N autosuggest-suggest _fake_as_suggest
zle -N autosuggest-disable _fake_as_disable
zle -N autosuggest-enable _fake_as_enable
zle -N autosuggest-accept _fake_as_accept
bindkey '^E' autosuggest-accept
