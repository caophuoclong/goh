# goh.zsh — inline completion through gohd, on Tab and as you type. Source from ~/.zshrc after compinit.
#
# Tab/Right with the menu open: insert the highlighted candidate (never runs the command).
# Up/Down move the highlight, wrapping. Shift+Tab: previous. Esc/Left: close (Left also moves normally).
# Tab with the menu closed uses native completion; GOH_LIVE=off enables Tab-triggered menus instead.
# Enter takes the highlighted candidate after menu navigation (or the only one) and runs the line (display.enterRuns);
# otherwise it uses the original shell binding. Ctrl+C ends the line; the inline menu is cleared before the next prompt.
# With GOH_LIVE=off any edit or cursor move closes the menu (below: as-you-type keeps it in step instead).
# Navigation keys are bound only while the menu is open.
#
# As-you-type (GOH_LIVE=on, the default; `off` = Tab only): after each edit, if the line is a recognised
# command in progress (its first word has a completion spec and every earlier word fits it; see
# docs/docs/18), the menu opens or updates by itself and follows what you type; when the line stops
# being valid or has no candidates it closes. Esc hides it until the line changes again. Unknown
# commands never open it by themselves (Tab still completes them). It reacts to typing and deleting
# only, not to history recall, and waits out bursts of input (paste, key repeat).
#
# Optional, off by default (`goh config`): with privacy.recordCommands the successful lines you run are kept in a
# private local file, so a started line can be finished from your history, the command that usually follows the
# last one is shown as ghost text at an empty prompt (Right or Tab takes it), and values you gave an option come
# back as suggestions. `goh forget` deletes the file.
#
# Fail-open: any problem (no daemon, timeout, odd values) falls through to the widget Tab was
# bound to before. `goh-unload` restores the original binding.
# Overlay: with GOH_DISPLAY=auto (default) or overlay, the menu is also sent to `goh-popup`, which draws
# it at the caret when the caret is verified (iTerm2, Terminal.app) and otherwise answers "inline".
# GOH_DISPLAY=inline never starts it. While starting the popup, no temporary inline list is drawn;
# an inline answer or a bounded startup timeout enables the fallback. Frames go over FIFOs; the answer is
# read asynchronously (zle -F), so a slow or dead popup can never block typing.
# Theme: ghost text uses the session's resolved `text-muted` (docs/design-system/theme-resolution.md);
# `goh-theme` shows how it was resolved, `goh-theme-refresh` asks the terminal again.
#
# Offsets on the wire are UTF-8 bytes; zsh's CURSOR and string slicing count characters, so
# conversions happen here with LC_ALL=C (byte semantics) and nowhere else.

autoload -Uz add-zle-hook-widget is-at-least

# Sourcing again (e.g. after rebuilding) starts clean: stop a popup left by the previous version.
(( ${+functions[_goh_pop_stop]} )) && _goh_pop_stop 2>/dev/null

typeset -g _GOH_CTL=${GOH_CTL:-goh}
typeset -g _GOH_SESSION="zsh-$$-$RANDOM"
# Lets `goh doctor` tell whether this very shell loaded the adapter (it compares the pid with its parent's).
typeset -gx GOH_ADAPTER="zsh:$$"
# Settings from the config file (`goh config`); an environment variable you set yourself still wins.
# _goh_from_config names the ones set here, so `goh reload` can read them again.
typeset -ga _goh_from_config=()
{
  local _goh_line _goh_name
  for _goh_line in ${(f)"$(command $_GOH_CTL config env 2>/dev/null)"}; do
    _goh_name=${_goh_line%%=*}
    [[ -n ${(P)_goh_name-} ]] || { typeset -g $_goh_name=${_goh_line#*=}; _goh_from_config+=($_goh_name) }
  done
}
typeset -gi _GOH_LIMIT=${GOH_LIMIT:-8}
(( _GOH_LIMIT < 1 )) && _GOH_LIMIT=1
typeset -g _GOH_DISPLAY=${GOH_DISPLAY:-auto}   # auto | overlay | inline
typeset -g _GOH_INLINE_STYLE=${GOH_INLINE_STYLE:-box}   # box | flat (how the menu looks when drawn in the terminal)
typeset -g _GOH_LIVE=${GOH_LIVE:-on}           # on | off (as-you-type suggestions)
typeset -gi _GOH_LIVE_MIN=${GOH_LIVE_MIN:-0}   # characters of the current word before the menu may open by itself
typeset -g _goh_seen_buf
typeset -gi _goh_seen_cur=-1 _goh_busy=0 _goh_told_permission=0 _goh_ssh_wrapped=0
# Widgets after which an edit counts as the user typing (not history recall, autosuggest accept, ...).
typeset -ga _goh_typing_widgets=(self-insert magic-space backward-delete-char delete-char vi-backward-delete-char vi-delete-char backward-kill-word kill-word bracketed-paste quoted-insert)
typeset -g _GOH_POPUP=${GOH_POPUP:-goh-popup}
typeset -g _GOH_ORIG_TAB
typeset -g _goh_pop_dir _goh_pop_pid _goh_items_json _goh_last_decision _goh_last_frame
typeset -gi _goh_pin=-1 _goh_pout=-1 _goh_overlay=0 _goh_win=1 _goh_menu_id=0
# _goh_overlay: -1 = awaiting the first popup answer, 0 = inline, 1 = overlay.
typeset -ga _goh_ranges
typeset -g _goh_new _goh_pre _goh_tail_out
typeset -gi _goh_on=0 _goh_sel=1 _goh_buf_cur=0 _goh_navigated=0
# The history line shown as ghost text (its index in the menu), which → takes; 0 = none.
typeset -gi _goh_ghost_line=0
typeset -g _goh_buf
typeset -ga _goh_start _goh_end _goh_cursor _goh_kind _goh_label _goh_insert _goh_desc _goh_value _goh_used
typeset -gA _goh_theme   # resolved design tokens (see goh-theme-refresh)
typeset -g _goh_ghost_style='fg=8'
typeset -gA _goh_saved   # key -> widget it was bound to before the menu opened ("" = unbound)
typeset -gA _goh_saved_str   # key -> text it typed before the menu opened (`bindkey -s`, as oh-my-zsh's Alt+L)

# Menu-only keys and the widget each gets; those starting with Esc are dispatched by _goh_esc (see below).
typeset -gA _goh_menu_keys=(
  '^[[A' _goh_up   '^[OA' _goh_up
  '^[[B' _goh_down '^[OB' _goh_down
  '^[[C' _goh_take '^[OC' _goh_take
  '^[[D' _goh_left '^[OD' _goh_left
  '^M' _goh_enter '^J' _goh_enter
  '^[[Z' _goh_up
  '^['   _goh_esc
)

# GOH_TRACE=/path/file logs what the adapter does on each keystroke (time, line, menu state, cost), to
# see how it feels without watching a screen. Off unless set.
zmodload -s zsh/datetime
_goh_trace() { [[ -n ${GOH_TRACE-} ]] && print -r -- "$EPOCHREALTIME $*" >> $GOH_TRACE; return 0 }

# Run another plugin's (or your own) widget from code that runs with no_unset, as it runs from its key: unset
# allowed. zsh-autosuggestions unsets POSTDISPLAY and then reads it; under no_unset that is an error printed into the
# line while it is being drawn, and the display falls apart.
_goh_zle() { setopt local_options unset; zle "$@" }

# Is INSERT (ending at the replace span's END byte) just what is already typed from START, plus at most
# a trailing space? Then offering it adds nothing. _goh_same_word BUFFER START END INSERT
_goh_same_word() {
  local LC_ALL=C
  local typed=${1[$2+1,$3]}
  [[ $4 == "$typed" || $4 == "$typed " ]]
}

# Byte length of $1 in REPLY.
_goh_bytes() { local LC_ALL=C; REPLY=${#1} }

# _goh_splice BUFFER START_BYTE END_BYTE INSERT CURSOR_BYTE -> _goh_new, _goh_pre
_goh_splice() {
  local LC_ALL=C
  _goh_new="${1[1,$2]}${4}${1[$3+1,-1]}"
  _goh_pre="${_goh_new[1,$5]}"
}

# _goh_tail BUFFER START_BYTE INSERT -> _goh_tail_out: what INSERT adds beyond what is already
# typed from START to the end of the line ("" when INSERT does not extend the typed text).
_goh_tail() {
  local LC_ALL=C
  local typed=${1[$2+1,-1]}
  if [[ ${3[1,${#typed}]} == "$typed" ]]; then _goh_tail_out=${3[${#typed}+1,-1]}; else _goh_tail_out=; fi
}

# ---- overlay (goh-popup) ------------------------------------------------------------------

_goh_pop_alive() { [[ -n $_goh_pop_pid ]] && kill -0 $_goh_pop_pid 2>/dev/null }

_goh_pop_stop() {
  if _goh_pop_alive; then
    (( _goh_pin >= 0 )) && print -r -u $_goh_pin -- '{"type":"quit"}' 2>/dev/null
  fi
  (( _goh_pout >= 0 )) && { zle -F $_goh_pout 2>/dev/null; exec {_goh_pout}<&-; }
  (( _goh_pin >= 0 )) && exec {_goh_pin}>&-
  _goh_pin=-1 _goh_pout=-1 _goh_overlay=0
  [[ -n $_goh_pop_dir ]] && rm -rf -- $_goh_pop_dir
  _goh_pop_dir= _goh_pop_pid=
}

# The popup's answers ("overlay" or "inline") arrive here whenever ZLE is idle.
_goh_on_decision() {
  emulate -L zsh
  local line delta=0 step event_id MATCH
  local -a match mbegin mend
  local was_inline=$(( _goh_overlay == 0 ))
  if [[ -n ${2-} ]]; then _goh_pop_stop; return 0; fi    # hup/nval: the popup went away
  while read -r -u $1 -t 0 line; do
    if [[ $line == *'"type":"popup_timeout"'* ]]; then
      (( _goh_overlay == -1 )) || continue  # an answered or stopped popup has no pending timeout
      line='{"type":"decision","display":"inline","reason":"popup startup timed out"}'
    fi
    if [[ $line == *'"type":"navigate"'* ]]; then
      # Only the currently displayed query may navigate. The shell owns the selection and edit.
      if (( _goh_overlay == 1 )) && _goh_valid &&
         [[ $line =~ '"id":"([a-zA-Z0-9-]+)"' ]]; then
        event_id=$match[1]
        if [[ $event_id == "goh-${_GOH_SESSION}-${_goh_menu_id}" &&
              $line =~ '"delta":(-?[0-9]{1,3})([,}])' ]]; then
          step=$match[1]
          (( step >= -100 && step <= 100 )) && (( delta += step ))
        fi
      fi
      continue
    fi
    _goh_last_decision=$line
    # Once per shell, and only where a popup was expected: the permission is the one thing the user can fix.
    if (( ! _goh_told_permission )) && [[ $line == *'"display":"inline"'*'not allowed to use the Accessibility'* && -z ${TMUX-}${STY-}${SSH_CONNECTION-} &&
         $TERM_PROGRAM == (iTerm.app|Apple_Terminal|vscode|claude-desktop) ]]; then
      _goh_told_permission=1
      # In the menu, not `zle -M`: that line goes at the next key, which is usually already being typed.
      if [[ ${GOH_POPUP_OWN_PERMISSION-} == on ]]; then
        _goh_notice="goh: Goh (the popup) is not allowed to use Accessibility, so menus are drawn inline. Switch Goh on in System Settings > Privacy & Security > Accessibility"
      else
        _goh_notice="goh: this terminal is not allowed to use Accessibility, so menus are drawn inline. Run: goh doctor --fix"
      fi
    fi
    _goh_trace "decision ${line[1,80]}"
    case $line in
      *'"display":"overlay"'*) _goh_overlay=1 ;;
      *'"display":"inline"'*)  _goh_overlay=0 ;;
    esac
  done
  if (( delta )) && _goh_valid && (( _goh_overlay == 1 )); then
    local next=$(( _goh_sel + delta ))
    (( next < 1 )) && next=1
    (( next > $#_goh_label )) && next=$#_goh_label
    (( next != _goh_sel )) && _goh_move $(( next - _goh_sel ))
  fi
  if _goh_valid && (( was_inline != (_goh_overlay == 0) )); then
    _goh_render
    zle -R  # Only a real switch to/from inline needs terminal redisplay.
  fi
  return 0
}

# Start the popup on first use. Returns non-zero (and leaves everything inline) on any problem.
_goh_pop_start() {
  _goh_pop_alive && return 0
  [[ -n $_goh_pop_pid ]] && return 1  # retry a failed popup on the next menu, not every navigation key
  _goh_pop_stop
  [[ $_GOH_DISPLAY == inline ]] && return 1
  local -a pop=($_GOH_POPUP)
  # Connected with `goh ssh`: the popup runs on the machine the terminal is on, reached through a forwarded socket.
  [[ -S ${LC_GOH_POPUP-} && -z ${TMUX-}${STY-} ]] && pop=($_GOH_CTL popup-bridge $LC_GOH_POPUP)
  command -v $pop[1] >/dev/null 2>&1 || return 1
  local dir
  dir=$(mktemp -d "${TMPDIR:-/tmp}/goh-popup.XXXXXX" 2>/dev/null) || return 1
  mkfifo $dir/in $dir/out 2>/dev/null || { rm -rf -- $dir; return 1 }
  local errf=/dev/null; [[ -n ${GOH_DEBUG-} ]] && errf=$dir/err   # GOH_DEBUG=1 keeps the popup's stderr
  # The config sets this as a shell variable, not exported: hand it to the popup, which reads it at start.
  GOH_POPUP_OWN_PERMISSION=${GOH_POPUP_OWN_PERMISSION-} command $pop <$dir/in >$dir/out 2>$errf &!
  _goh_pop_pid=$!
  _goh_pop_dir=$dir
  # Read-write opens never block, and keep a reader on `in` so a dead popup cannot raise SIGPIPE.
  if ! { exec {_goh_pin}<>$dir/in {_goh_pout}<>$dir/out } 2>/dev/null; then _goh_pop_stop; return 1; fi
  # The decision handler reads BUFFER/CURSOR and updates the display, so it must run as a widget.
  zle -N _goh_on_decision
  zle -F -w $_goh_pout _goh_on_decision
  _goh_overlay=-1
  # Wake the same event handler if a live-but-unresponsive popup never answers. This bounded
  # child never reads the tty, and its inherited FD belongs to this popup even after a reload.
  ( command sleep 1; print -r -u $_goh_pout -- '{"type":"popup_timeout"}' ) 2>/dev/null &!
}

# JSON string escaping for single-line fields.
_goh_json() { REPLY=${1//\\/\\\\}; REPLY=${REPLY//\"/\\\"} }

# Static part of the frame: the items, built once per menu.
_goh_build_items() {
  local i l d r p
  local -a parts rp pairs
  for (( i = 1; i <= $#_goh_label; i++ )); do
    _goh_json "${_goh_label[i]}"; l=$REPLY
    _goh_json "${_goh_desc[i]:-${_goh_kind[i]}}"; d=$REPLY
    pairs=()
    rp=("${(@s:,:)_goh_ranges[i]}")
    for p in $rp; do [[ $p == <->-<-> ]] && pairs+=("[${p/-/,}]"); done
    parts+=("{\"label\":\"$l\",\"description\":\"$d\",\"match_ranges\":[${(j:,:)pairs}]}")
  done
  _goh_items_json="[${(j:,:)parts}]"
}

# Tell the popup what to draw (selection and window change on every move).
_goh_pop_show() {
  [[ $_GOH_DISPLAY == inline ]] && return 1
  _goh_pop_alive || _goh_pop_start || return 1
  _goh_window
  local enter=false; _goh_enter_takes && enter=true
  local theme= k ctx cell=
  for k in surface-base surface-selected border-subtle text-primary text-muted accent text-match; do
    [[ ${_goh_theme[$k]-} =~ '^#[0-9a-f]{6}$' ]] && theme+="${theme:+,}\"$k\":\"${_goh_theme[$k]}\""
  done
  [[ ${_goh_theme[cell-height]-} =~ '^[0-9]+(\.[0-9]+)?$' ]] && cell=",\"cell_height\":${_goh_theme[cell-height]}"
  # A nested scalar expansion indexes a character when (z) produces just one word.
  # Keep tokens in an array so a bare command has the same identity as "command arg".
  local -a words=("${(@z)BUFFER}")
  _goh_json "${(Q)words[1]}"; ctx=$REPLY
  _goh_trace "frame sel=$_goh_sel n=$#_goh_label"
  _goh_last_frame="{\"type\":\"show\",\"state\":{\"id\":\"goh-${_GOH_SESSION}-${_goh_menu_id}\",\"items\":$_goh_items_json,\"selected\":$(( _goh_sel - 1 )),\"window_start\":$(( _goh_win - 1 )),\"context\":\"$ctx\"},\"anchor\":\"auto\",\"mode\":\"$_GOH_DISPLAY\",\"max_rows\":$_GOH_LIMIT,\"enter_accepts\":$enter,\"theme\":{$theme}$cell}"
  print -r -u $_goh_pin -- "$_goh_last_frame" 2>/dev/null
}

_goh_pop_hide() {
  _goh_trace hide
  _goh_pop_alive && (( _goh_pin >= 0 )) && print -r -u $_goh_pin -- '{"type":"hide"}' 2>/dev/null
}

# ---- late answers ----------------------------------------------------------------------------
# A program the menu asked (Docker, a cluster, tmux, ...) had not answered: ask again in 300 ms while the line stays
# the same, so its values show without another key. One timer at a time, at most 16 tries (about 5 s) per line.
typeset -gi _goh_late_fd=-1 _goh_late_tries=0 _goh_late=0
typeset -g _goh_late_line

_goh_late_stop() {
  (( _goh_late_fd >= 0 )) || return 0
  zle -F $_goh_late_fd 2>/dev/null
  exec {_goh_late_fd}<&-
  _goh_late_fd=-1
}

# Stop waiting: the footer's `…` goes away (the menu is drawn again if it is still open).
_goh_late_giveup() {
  _goh_late_stop
  (( _goh_late )) || return 0
  _goh_late=0
  if _goh_valid; then _goh_render; _goh_pop_show; fi
}

_goh_late_wait() {
  if [[ $_goh_late_line != "$CURSOR:$BUFFER" ]]; then _goh_late_line="$CURSOR:$BUFFER" _goh_late_tries=0; fi
  (( _goh_late_fd >= 0 )) && return 0
  if (( ++_goh_late_tries > 16 )); then _goh_late_giveup; return 0; fi
  exec {_goh_late_fd}< <(command sleep 0.3; print)
  zle -F -w $_goh_late_fd _goh_on_late
}

# zle does not look at a Ctrl-C that arrives while a handler runs (the re-ask below waits on `goh` for a moment,
# several times a second): it would act on it only at the next key, so Ctrl-C seemed to do nothing. Catch it here and
# raise it again once zle is waiting for input, where it aborts the line as usual.
_goh_on_late() {
  emulate -L zsh
  setopt localtraps
  local interrupted=0
  trap 'interrupted=1' INT
  _goh_on_late_ask
  (( interrupted )) && { command sleep 0.05; kill -INT $$ } &!
}

_goh_on_late_ask() {
  _goh_late_stop
  [[ $_goh_late_line == "$CURSOR:$BUFFER" && $CONTEXT == start ]] || return 0
  # A refresh would put the selection back on the first row: once the user has moved in the menu, stop asking.
  if (( _goh_on && _goh_navigated )); then _goh_late_giveup; zle -R; return 0; fi
  _goh_live_refresh
  zle -R
}

# ---- menu state ---------------------------------------------------------------------------

# zsh-autosuggestions shows its suggestion in POSTDISPLAY, where the menu and its ghost text are, and its answer
# can arrive after ours, so the menu would vanish. While the menu is open the plugin is paused (unless you turned it
# off yourself), and its keys that take a suggestion into the line (End, Ctrl+E, Alt+F, autosuggest-accept, ...)
# close the menu first, or they would take the menu's text instead.
typeset -gi _goh_as_paused=0 _goh_as_found=0
typeset -ga _goh_as_keys _goh_bound
# Esc also starts the arrow keys' sequences and the Alt bindings (^[b, ^[[200~ ...), so while any of them is bound
# ZLE waits KEYTIMEOUT (0.4 s by default, read once per line) after Esc before it can run _goh_esc. While the menu is
# open those are unbound and _goh_esc reads the rest of a sequence itself; these are the ones it knows.
typeset -ga _goh_esc_keys

_goh_as_find_keys() {
  (( ${+widgets[autosuggest-accept]} )) || return 0
  local -a takers=(autosuggest-accept autosuggest-execute autosuggest-partial-accept
    ${ZSH_AUTOSUGGEST_ACCEPT_WIDGETS-} ${ZSH_AUTOSUGGEST_PARTIAL_ACCEPT_WIDGETS-} ${ZSH_AUTOSUGGEST_EXECUTE_WIDGETS-})
  local line; local -a w
  for line in "${(@f)$(bindkey -L)}"; do
    w=(${(z)line})   # bindkey "KEY" WIDGET (ranges, `bindkey -R`, are never these widgets)
    (( $#w == 3 && ${takers[(Ie)$w[3]]} )) && _goh_as_keys+=("${(Q)w[2]}")
  done
}

_goh_keys_on() {
  local k
  (( _goh_as_found )) || { _goh_as_found=1; _goh_as_find_keys }
  local line; local -a w; local -A now str   # key -> widget, or -> the text a `bindkey -s` key types
  for line in "${(@f)$(bindkey -L)}"; do
    w=(${(z)line})   # bindkey "KEY" WIDGET, or bindkey -s "KEY" "TEXT" (ranges, `bindkey -R`, are never Esc sequences)
    if (( $#w == 3 )); then now[${(Q)w[2]}]=$w[3]
    elif (( $#w == 4 )) && [[ $w[2] == -s ]]; then now[${(Q)w[3]}]= str[${(Q)w[3]}]=${(Q)w[4]}
    fi
  done
  local -a menu=(${(k)_goh_menu_keys})
  _goh_esc_keys=(${(M)${(k)now}:#\^\[?*} ${(M)menu:#\^\[?*})
  _goh_bound=($menu ${_goh_as_keys:|menu} ${_goh_esc_keys:|menu})
  for k in $_goh_bound; do
    _goh_saved[$k]=${now[$k]-} _goh_saved_str[$k]=${str[$k]-}
    if [[ $k == '^['?* ]]; then
      bindkey -r -- $k   # reached through _goh_esc
    else
      bindkey -- $k ${_goh_menu_keys[$k]:-_goh_left}   # _goh_left: close, then the key's own widget
    fi
  done
  if (( ${+widgets[autosuggest-disable]} && ! ${+_ZSH_AUTOSUGGEST_DISABLED} )); then
    _goh_as_paused=1
    _goh_zle autosuggest-disable
    # An answer already on its way (async mode) would still land in POSTDISPLAY. Cancel it as the plugin cancels
    # one for a newer request; with another version that keeps no such fd, it can still blink the menu away once.
    if [[ -n ${_ZSH_AUTOSUGGEST_ASYNC_FD-} ]] && { true <&$_ZSH_AUTOSUGGEST_ASYNC_FD } 2>/dev/null; then
      zle -F $_ZSH_AUTOSUGGEST_ASYNC_FD
      exec {_ZSH_AUTOSUGGEST_ASYNC_FD}<&-
      [[ -n ${_ZSH_AUTOSUGGEST_CHILD_PID-} ]] && kill -TERM $_ZSH_AUTOSUGGEST_CHILD_PID 2>/dev/null
      _ZSH_AUTOSUGGEST_ASYNC_FD=
    fi
  fi
}

_goh_keys_off() {
  local k
  for k in $_goh_bound; do
    if [[ -n ${_goh_saved[$k]-} ]]; then bindkey -- $k ${_goh_saved[$k]}
    elif [[ -n ${_goh_saved_str[$k]-} ]]; then bindkey -s -- $k ${_goh_saved_str[$k]}
    else bindkey -r -- $k; fi
  done
  _goh_saved=() _goh_saved_str=() _goh_bound=() _goh_esc_keys=()
  if (( _goh_as_paused )); then
    _goh_as_paused=0
    _goh_zle autosuggest-enable
  fi
}

_goh_clear_ghost() {
  POSTDISPLAY= _goh_rows_shown=0
  # ZLE normalises colors and moves ranges when BUFFER changes. Its memo tag survives both;
  # remove only our highlights, leaving syntax-highlighting and other plugins' regions intact.
  # Also a highlight that starts past the line: it was for the text shown after it (a zsh-autosuggestions
  # suggestion), which is gone now. Left in place, it would dim the next text accepted there.
  local h; local -a keep
  for h in "${region_highlight[@]}"; do
    [[ $h == *memo=goh-ghost ]] && continue
    [[ $h == <->' '* ]] && (( ${h%% *} >= ${#BUFFER} )) && continue
    keep+=("$h")
  done
  region_highlight=("${keep[@]}")
}

# Remove the menu message (zsh < 5.9 only), and only if one is shown: every `zle -M` makes ZLE redraw
# below the line, and two in one refresh (accepting a directory closes, then reopens) leave it one row too
# high, so a multiline prompt is overwritten.
typeset -gi _goh_msg=0
# A one-time notice drawn under the inline menu until that menu closes (see _goh_on_decision).
typeset -g _goh_notice=
typeset -gi _goh_notice_shown=0
_goh_clear_msg() { (( _goh_msg )) || return 0; _goh_msg=0; zle -M '' }

_goh_close() {
  [[ -n $_goh_pop_pid ]] && ! _goh_pop_alive && _goh_pop_stop
  (( _goh_on )) || return 0
  _goh_on=0 _goh_navigated=0 _goh_ghost_line=0
  (( _goh_notice_shown )) && _goh_notice= _goh_notice_shown=0
  _goh_clear_ghost
  # ZLE owns message removal at line-init/line-finish. A -M refresh there leaves its
  # next-prompt display cache stale (the last line of a multiline prompt disappears).
  if [[ ${1-} == no-redisplay ]]; then _goh_msg=0; else _goh_clear_msg; fi
  _goh_pop_hide
  _goh_keys_off
}

# Menu is only valid while the line is exactly as it was when it opened.
_goh_valid() { (( _goh_on )) && [[ $BUFFER == "$_goh_buf" && $CURSOR == $_goh_buf_cur ]] }

# Keep both inline and native views on the same slice of the full result list.
_goh_window() {
  local rows=$(( $#_goh_label < _GOH_LIMIT ? $#_goh_label : _GOH_LIMIT ))
  if (( _goh_sel < _goh_win )); then _goh_win=$_goh_sel
  elif (( _goh_sel >= _goh_win + rows )); then _goh_win=$(( _goh_sel + 1 - rows )); fi
}

# The first $2 bytes of $1 in REPLY; the caller counts them in characters.
_goh_byte_prefix() { local LC_ALL=C; REPLY=${1[1,$2]} }

# The screen column (from 0) where the word being completed starts: the width of the prompt's last line plus the
# line typed before the word. A prompt this cannot measure only moves the box; nothing else depends on it.
# With prompt_subst the prompt is measured as zsh draws it: parameters first, then `%` escapes. One that runs a
# command (`$(...)`, a backtick) is never run again here (it may be slow or have side effects) and counts as no
# width, so the box anchors at the word's offset in the typed line.
_goh_word_column() {
  local subst=${options[prompt_subst]}
  emulate -L zsh
  setopt extended_glob
  local p before i start=0
  if [[ $subst == on ]]; then
    if [[ $PS1 == *'$('* || $PS1 == *'`'* ]]; then p=; else p=${(%)${(e)PS1}}; fi
  else
    p=${(%)PS1}
  fi
  # The word being completed starts where the first non-history candidate starts: a history line starts at 0,
  # and the box must not jump when the selection moves onto one.
  for (( i = 1; i <= $#_goh_kind; i++ )); do
    [[ ${_goh_kind[i]} == history ]] || { start=${_goh_start[i]:-0}; break }
  done
  p=${p//$'\e'\[[0-9;?]#[@-~]/}            # colours and other CSI sequences take no room
  p=${p//$'\e'\][^$'\a']#$'\a'/}           # nor do OSC sequences (titles, marks)
  p=${p##*$'\n'}
  _goh_byte_prefix "$BUFFER" $start; before=$REPLY
  if [[ $before == *$'\n'* ]]; then p= before=${before##*$'\n'}; fi
  REPLY=$(( (${(m)#p} + ${(m)#before}) % ${COLUMNS:-80} ))
}

# Kind glyph and colour of candidate $1 -> _goh_g, _goh_gs (docs/design-system/README.md, Iconography).
typeset -g _goh_g _goh_gs
_goh_glyph() {
  case ${_goh_kind[$1]} in
    command)    _goh_g='⌘' _goh_gs=fg=cyan ;;
    subcommand) _goh_g='↳' _goh_gs=fg=magenta ;;
    option)     if [[ -n ${_goh_value[$1]} ]]; then _goh_g='◇' _goh_gs=fg=blue; else _goh_g='⚑' _goh_gs=fg=yellow; fi ;;
    argument)   _goh_g='●' _goh_gs=fg=green ;;
    directory)  _goh_g='▸' _goh_gs=fg=blue ;;
    file)       _goh_g='◦' _goh_gs= ;;
    history)    _goh_g='↺' _goh_gs=$_goh_ghost_style ;;
    *)          _goh_g='·' _goh_gs= ;;
  esac
}

# Inline menu layout (docs/design-system/README.md, Inline menu): rows of marker, glyph, label and any
# warning; the selected suggestion's description; position and key hints. Shapes, not colour, carry
# every cue, so NO_COLOR keeps all of it but the colours.
_goh_render() {
  local i k n p s e h row label shown risk desc room pad
  local -a rows hl
  local width=$(( ${COLUMNS:-80} - 1 )) box=0
  # The box needs room for its two sides and a menu worth reading.
  if [[ $_GOH_INLINE_STYLE == box ]] && (( width >= 24 )); then box=1; width=$(( width - 2 > 60 ? 60 : width - 2 )); fi
  local muted=$_goh_ghost_style accent=fg=cyan match=bold,underline warn=bold,fg=red
  [[ ${_goh_theme[accent]-} =~ '^#[0-9a-f]{6}$' ]] && accent="fg=${_goh_theme[accent]}"
  [[ ${_goh_theme[text-match]-} =~ '^#[0-9a-f]{6}$' ]] && match+=",fg=${_goh_theme[text-match]}"
  local color=1
  if [[ -n ${NO_COLOR-} ]]; then color=0 muted= accent= match=bold,underline warn=bold; fi
  _goh_window
  local end=$(( _goh_win + _GOH_LIMIT - 1 ))
  (( end > $#_goh_label )) && end=$#_goh_label

  # One label column for the whole list, so it does not shift while scrolling.
  local col=1
  for label in "${_goh_label[@]}"; do (( ${#label} > col )) && col=${#label}; done
  (( col > 32 )) && col=32
  (( col > width - 4 )) && col=$(( width - 4 ))
  (( col < 1 )) && col=1

  # The first three suggestions picked before (only ever set while learned choices are on).
  local -A badge
  local -a marks=('①' '②' '③')
  n=0
  for (( i = 1; i <= $#_goh_label && n < 3; i++ )); do [[ -n ${_goh_used[i]} ]] && badge[$i]=${marks[++n]}; done

  # hl entries: "ROW START END STYLE", character offsets within the row.
  for (( i = _goh_win; i <= end; i++ )); do
    k=$(( $#rows + 1 ))
    _goh_glyph $i
    label=${_goh_label[i]} shown=${_goh_label[i]}
    (( ${#shown} > col )) && shown="${shown[1,col-1]}…"
    if (( i == _goh_sel )); then
      row="❯ ${badge[$i]:-$_goh_g} $shown"
      hl+=("$k 0 1 bold${accent:+,$accent}" "$k 4 $(( 4 + ${#shown} )) bold${accent:+,$accent}")
    else
      row="  ${badge[$i]:-$_goh_g} $shown"
    fi
    (( color )) && [[ -n $_goh_gs ]] && hl+=("$k 2 3 $_goh_gs")
    for p in ${(s:,:)_goh_ranges[i]}; do
      [[ $p == <->-<-> ]] || continue
      _goh_byte_prefix "$label" ${p%-*}; s=${#REPLY}
      _goh_byte_prefix "$label" ${p#*-}; e=${#REPLY}
      (( ${#shown} < ${#label} && e > ${#shown} - 1 )) && e=$(( ${#shown} - 1 ))
      (( s < e )) && hl+=("$k $(( 4 + s )) $(( 4 + e )) $match")
    done
    # A warning shows on every row, not only the selected one: it should be seen before it is picked.
    desc=${_goh_desc[i]}
    if [[ $desc == '⚠'* ]]; then
      risk=${desc%% — *}
      pad=$(( 4 + col + 2 ))
      room=$(( width - pad ))
      if (( room > 3 )); then
        (( ${#risk} > room )) && risk="${risk[1,room-1]}…"
        row="${(r:pad:)row}$risk"
        hl+=("$k $pad $(( pad + ${#risk} )) $warn")
      fi
    fi
    rows+=("${row[1,width]}")
  done

  # The selected suggestion's description, without the warning already on its row.
  desc=${_goh_desc[_goh_sel]}
  if [[ $desc == '⚠'* ]]; then
    if [[ $desc == *' — '* ]]; then desc=${desc#* — }; else desc=; fi
  fi
  desc="  ${desc:-${_goh_kind[_goh_sel]}}"
  (( ${#desc} > width )) && desc="${desc[1,width-1]}…"
  rows+=("$desc")
  hl+=("$#rows 0 ${#desc} $muted")

  # Until you move in the menu, the best whole line from history shows as ghost text, as with zsh-autosuggestions:
  # → takes that line, Tab the selected suggestion. A history line's ghost is its label: a secret in it is masked
  # there (`TOKEN=****`), and only taking it inserts the line itself.
  _goh_ghost_line=0
  if (( ! _goh_navigated && CURSOR == ${#BUFFER} )); then
    _goh_bytes "$BUFFER"
    for (( i = 1; i <= $#_goh_kind; i++ )); do
      if [[ ${_goh_kind[i]} == history && ${_goh_start[i]} == 0 && ${_goh_end[i]} == $REPLY && ${_goh_label[i]} == "$BUFFER"?* ]]; then
        _goh_ghost_line=$i
        break
      fi
    done
  fi
  local pos="$_goh_sel/$#_goh_label" accept_hint="Tab/→"
  (( _goh_ghost_line && _goh_ghost_line != _goh_sel )) && accept_hint="→ line · Tab"
  (( $#_goh_label >= 50 )) && pos+='+'   # menus ask for 50: there may be more
  (( _goh_late )) && pos+=' …'   # a program has not answered yet: more may come
  local run_hint=
  if _goh_enter_takes; then
    if _goh_enter_runs; then run_hint=" · Enter run"; else accept_hint="Tab/Enter/→"; fi
  fi
  row="  $pos · $accept_hint insert$run_hint · ↑↓ move · Esc/← close"
  (( ${#row} > width )) && row="  $pos · ↑↓ $accept_hint Esc"
  (( ${#row} > width )) && row="  $pos"
  rows+=("${row[1,width]}")
  hl+=("$#rows 0 ${#row} $muted")

  # The notice, wrapped at word boundaries.
  if [[ -n $_goh_notice ]]; then
    row=
    for p in ${=_goh_notice}; do
      if [[ -n $row ]] && (( ${#row} + 1 + ${#p} > width )); then rows+=("$row"); row=; fi
      row+="${row:+ }$p"
    done
    rows+=("${row[1,width]}")
    (( _goh_overlay == 0 )) && _goh_notice_shown=1
  fi

  # Box style: the same rows inside a frame, filled with the surface colour and moved under the word being
  # completed, as far right as the terminal allows. Highlight offsets move with the rows.
  if (( box )); then
    local inner=1 r indent surface= selected= border=
    for r in "${rows[@]}"; do (( ${(m)#r} > inner )) && inner=${(m)#r}; done
    _goh_word_column; indent=$REPLY
    (( indent + inner + 2 > ${COLUMNS:-80} - 1 )) && indent=$(( ${COLUMNS:-80} - 1 - inner - 2 ))
    (( indent < 0 )) && indent=0
    if (( color )); then
      [[ ${_goh_theme[surface-base]-} =~ '^#[0-9a-f]{6}$' ]] && surface="bg=${_goh_theme[surface-base]}"
      [[ ${_goh_theme[surface-selected]-} =~ '^#[0-9a-f]{6}$' ]] && selected="bg=${_goh_theme[surface-selected]}"
      border=${muted:-fg=8}
    fi
    local edge= lead=
    edge=${(l:inner::─:)edge} lead=${(l:indent:: :)lead}
    local -a framed=("$lead╭$edge╮") moved=() clen=() f
    # Padded to `inner` screen cells: a wide or combining character makes a row's character count differ from its
    # width, so offsets on the right (the border, the fills) come from each framed row's own length.
    for r in "${rows[@]}"; do framed+=("$lead│${(mr:inner:)r}│"); done
    framed+=("$lead╰$edge╯")
    local last=$#framed sel_row=$(( _goh_sel - _goh_win + 2 ))
    for (( k = 1; k <= last; k++ )); do clen[k]=${#framed[k]}; done
    # Fills first, then the row styles. zsh 5.9 does not keep an earlier entry's background under a later entry
    # that sets a colour, so every style drawn on a fill repeats that fill's background.
    if [[ -n $surface ]]; then
      for (( k = 1; k <= last; k++ )); do moved+=("$k $indent $clen[k] $surface"); done
    fi
    [[ -n $selected ]] && moved+=("$sel_row $(( indent + 1 )) $(( clen[sel_row] - 1 )) $selected")
    if [[ -n $border ]]; then
      local b=$border${surface:+,$surface}
      moved+=("1 $indent $clen[1] $b" "$last $indent $clen[last] $b")
      for (( k = 2; k < last; k++ )); do moved+=("$k $indent $(( indent + 1 )) $b" "$k $(( clen[k] - 1 )) $clen[k] $b"); done
    fi
    local fill
    for h in "${hl[@]}"; do
      f=(${=h})
      fill=$surface
      [[ -n $selected ]] && (( f[1] + 1 == sel_row )) && fill=$selected
      moved+=("$(( f[1] + 1 )) $(( f[2] + indent + 1 )) $(( f[3] + indent + 1 ))${f[4]:+ ${fill:+$fill,}$f[4]}")
    done
    rows=("${framed[@]}") hl=("${moved[@]}")
  fi

  # Ghost text only where it is honest: cursor at the end and the insertion extends what is typed.
  _goh_clear_ghost
  local ghost= buflen
  _goh_bytes "$BUFFER"; buflen=$REPLY
  if [[ ${_goh_kind[_goh_sel]} == history ]]; then
    [[ ${_goh_label[_goh_sel]} == "$BUFFER"?* && CURSOR -eq ${#BUFFER} ]] && ghost=${_goh_label[_goh_sel]#"$BUFFER"}
  elif (( _goh_end[_goh_sel] == buflen && CURSOR == ${#BUFFER} )); then
    _goh_tail "$BUFFER" ${_goh_start[_goh_sel]} "${_goh_insert[_goh_sel]}"
    ghost=$_goh_tail_out
  fi
  (( _goh_ghost_line )) && ghost=${_goh_label[_goh_ghost_line]#"$BUFFER"}

  # Inline menu: drawn as part of the line (POSTDISPLAY, below the cursor) with real styles, so it looks the
  # same in any terminal, including the ones that cannot host the overlay (VS Code, JetBrains, Claude).
  # `zle -M` would print escape codes literally. Needs the memo tag (zsh 5.9+) to remove only our styles.
  local styled=0
  if (( _goh_overlay != 0 )) && _goh_pop_alive; then
    _goh_clear_msg
  else
    _goh_overlay=0
    if is-at-least 5.9; then styled=1; _goh_clear_msg; else zle -M "${(F)rows}"; _goh_msg=1; fi
  fi

  # "P" offsets count from the start of PREDISPLAY+BUFFER+POSTDISPLAY.
  local from=$(( ${#PREDISPLAY} + ${#BUFFER} ))
  local -a at=(0) f
  POSTDISPLAY=$ghost
  if [[ -n $ghost ]] && is-at-least 5.9; then
    # Older zsh cannot identify a plugin's highlights reliably. Keep its ghost text unstyled
    # rather than leave a stale color on accepted text or remove someone else's highlighting.
    region_highlight+=("P$from $(( from + ${#ghost} )) $_goh_ghost_style memo=goh-ghost")
  fi
  if (( styled )); then
    POSTDISPLAY+=$'\n'"${(pj:\n:)rows}"
    _goh_rows_shown=$#rows
    p=$(( from + ${#ghost} + 1 ))
    for row in "${rows[@]}"; do at+=($p); p=$(( p + ${#row} + 1 )); done
    for h in "${hl[@]}"; do
      f=(${=h})
      (( $#f == 4 )) || continue            # no style (NO_COLOR, unresolved theme)
      e=${#rows[f[1]]}
      (( f[3] < e )) && e=$f[3]
      (( f[2] < e )) && region_highlight+=("P$(( at[f[1]+1] + f[2] )) $(( at[f[1]+1] + e )) $f[4] memo=goh-ghost")
    done
  fi
}
# Apply candidate $1 to the line; fails (leaving the line alone) on any inconsistency.
_goh_apply() {
  local i=$1 buflen before=$BUFFER
  _goh_bytes "$BUFFER"; buflen=$REPLY
  (( _goh_start[i] <= _goh_end[i] && _goh_end[i] <= buflen && _goh_cursor[i] >= _goh_start[i] )) || return 1
  [[ -n ${_goh_insert[i]} ]] || return 1
  _goh_splice "$BUFFER" ${_goh_start[i]} ${_goh_end[i]} "${_goh_insert[i]}" ${_goh_cursor[i]}
  BUFFER=$_goh_new
  CURSOR=${#_goh_pre}
  # Remember the pick (command, subcommand and label only) so it ranks first next time.
  { local LC_ALL=C; command $_GOH_CTL learn --cwd $PWD --prefix "${before[1,${_goh_start[i]}]}" --label "${_goh_label[i]}" >/dev/null 2>&1 &! } 2>/dev/null
}

# ---- widgets ------------------------------------------------------------------------------

_goh_move() {  # _goh_move DELTA ORIG_WIDGET
  if _goh_valid; then
    _goh_navigated=1
    _goh_sel=$(( (_goh_sel - 1 + $1 + $#_goh_label) % $#_goh_label + 1 ))
    _goh_render
    zle -R            # finish the redraw first: the popup reads the terminal cursor when it gets the frame
    _goh_pop_show
  else
    _goh_close
    zle ${2:-undefined-key}
  fi
}
_goh_down() { _goh_move 1 down-line-or-history }
_goh_up()   { _goh_move -1 up-line-or-history }

# Esc, alone or starting a sequence. The terminal writes a key's sequence at once, so what follows Esc is read while
# it can still become a known key, waiting at most 20 ms for each byte. Up/Down/Right act on the menu; a lone Esc
# closes it, and stays closed: the hook refreshes only when the line changes. Anything else closes it and is typed
# again into the keys as they were, so Left, Alt+key, a paste or an unknown key does what it did without the menu.
_goh_esc() {
  local seq='^[' raw=$'\e' c k more
  while :; do
    more=0
    for k in $_goh_esc_keys; do [[ $k == "$seq"?* ]] && { more=1; break }; done
    (( more )) || break
    read -s -k 1 -t 0.02 c || break
    seq+=${(V)c} raw+=$c
  done
  case ${_goh_menu_keys[$seq]-} in
    (_goh_up|_goh_down) zle ${_goh_menu_keys[$seq]} ;;
    (_goh_take) _goh_take || { _goh_close; zle -U -- $raw } ;;
    (*) _goh_close; if [[ $seq == '^[' ]]; then _goh_late_giveup; zle -R; else zle -U -- $raw; fi ;;
  esac
}

# All acceptance keys share the same validated edit and dismissal. Only Enter then runs the line (see _goh_enter).
# A directory is accepted without a trailing space, so its contents are what comes next: the menu reopens for them
# (one refresh after the line is redrawn, like after typing). Nothing else reopens by itself after accepting.
typeset -gi _goh_reopen=0
_goh_accept() {
  _goh_valid || return 1
  local dir=0; [[ ${_goh_insert[_goh_sel]} == */ ]] && dir=1
  _goh_apply $_goh_sel || { _goh_close; return 1 }
  _goh_close
  _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR   # only a later edit may reopen the menu
  [[ $_GOH_LIVE == on ]] && _goh_reopen=$dir
  return 0
}

# Enter takes the highlighted candidate after navigation, or the only one, unless taking it would
# change no more than a trailing space (the line is already complete: run it).
_goh_enter_takes() {
  (( _goh_navigated )) && return 0
  (( $#_goh_label == 1 )) || return 1
  local _goh_new _goh_pre
  _goh_splice "$BUFFER" ${_goh_start[1]} ${_goh_end[1]} "${_goh_insert[1]}" ${_goh_cursor[1]}
  [[ ${_goh_new% } != "${BUFFER% }" ]]
}

# Does Enter run the line after taking the highlighted candidate? Not when the line cannot be complete yet.
_goh_enter_runs() {
  [[ ${GOH_ENTER_RUNS:-on} == on && ${_goh_value[_goh_sel]-} != 1 && ${_goh_insert[_goh_sel]} != */ ]]
}

# Enter takes the candidate as above and runs the line (GOH_ENTER_RUNS, on unless set off), unless the line cannot be
# complete yet: an option that wants its value, or a directory whose contents come next, is only inserted (as Tab
# does). An untouched menu with several choices must not change a command being submitted. Running delegates to the
# user's saved widget, including plugin wrappers.
_goh_enter() {
  local key=${(V)KEYS}
  local original=${_goh_saved[$key]:-accept-line}
  if _goh_valid && _goh_enter_takes; then
    local runs=0
    _goh_enter_runs && runs=1
    if _goh_accept; then
      (( runs )) || return 0
      _goh_reopen=0
    fi
  fi
  # Line-finish owns cleanup when the original widget submits; do not force a refresh here.
  zle $original
}

# → takes what the ghost text shows: the history line, while one is shown.
_goh_take() {
  (( _goh_ghost_line && ! _goh_navigated )) && _goh_valid && _goh_sel=$_goh_ghost_line
  _goh_accept
}

_goh_left() {
  local key=${(V)KEYS}
  local original=${_goh_saved[$key]:-backward-char}
  _goh_close
  _goh_late_giveup
  zle $original
}

# Parse `goh menu` output ($1) into the _goh_* arrays; REPLY = number of candidates.
_goh_parse_menu() {
  (( ++_goh_menu_id ))
  _goh_navigated=0
  local line; local -a f
  _goh_start=() _goh_end=() _goh_cursor=() _goh_kind=() _goh_label=() _goh_insert=() _goh_desc=() _goh_ranges=() _goh_value=() _goh_used=()
  for line in "${(@f)1}"; do
    f=("${(@ps:\x1f:)line}")
    [[ $f[1] == <-> && $f[2] == <-> && $f[3] == <-> ]] || continue
    _goh_start+=($f[1]) _goh_end+=($f[2]) _goh_cursor+=($f[3])
    _goh_kind+=("$f[4]") _goh_label+=("$f[5]") _goh_insert+=("$f[6]") _goh_desc+=("$f[7]") _goh_ranges+=("$f[8]")
    _goh_value+=("${f[9]-}") _goh_used+=("${f[10]-}")   # absent from an older goh
  done
  REPLY=$#_goh_label
}

_goh_tab() {
  emulate -L zsh
  setopt no_unset

  _goh_accept_pred && return 0
  _goh_accept && return 0
  _goh_close

  # In live mode typing owns opening the menu. Tab only accepts an open menu or completes natively.
  if [[ $_GOH_LIVE == on ]]; then
    _goh_zle ${_GOH_ORIG_TAB:-expand-or-complete}
    return 0
  fi

  if [[ $CONTEXT == start && -n ${BUFFER//[[:space:]]/} ]]; then
    _goh_bytes "${BUFFER[1,CURSOR]}"
    local out
    out=$(command $_GOH_CTL menu --labels --session $_GOH_SESSION --cwd $PWD --limit 50 \
          --cursor-byte $REPLY --buffer "$BUFFER" 2>/dev/null) || out=
    if [[ -n $out ]]; then
      _goh_parse_menu "$out"
      if (( $#_goh_label == 1 )); then
        _goh_apply 1 && { _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR; return 0 }
      elif (( $#_goh_label > 1 )); then
        _goh_on=1 _goh_sel=1 _goh_win=1 _goh_buf=$BUFFER _goh_buf_cur=$CURSOR
        _goh_keys_on
        _goh_build_items
        [[ $_GOH_DISPLAY == inline ]] || _goh_pop_start
        _goh_render
        zle -R
        _goh_pop_show
        return 0
      fi
    fi
  fi
  _goh_zle ${_GOH_ORIG_TAB:-expand-or-complete}
}

# Resolve the theme for this session. With a terminal attached it asks for the default colours
# (OSC 10/11) via goh, which owns the bounded, filtered tty read. Run between prompts, never
# from a widget: replies arrive as terminal input.
goh-theme-refresh() {
  local out line
  if [[ -t 0 && -t 1 ]]; then out=$(command $_GOH_CTL theme --probe 2>/dev/null); else out=$(command $_GOH_CTL theme 2>/dev/null); fi
  _goh_theme=()
  for line in "${(@f)out}"; do _goh_theme[${line%% *}]=${line#* }; done
  _goh_ghost_style='fg=8'
  if [[ ${_goh_theme[text-muted]-} =~ '^#[0-9a-f]{6}$' ]]; then
    [[ $COLORTERM == (truecolor|24bit) ]] || zmodload -s zsh/nearcolor
    _goh_ghost_style="fg=${_goh_theme[text-muted]}"
  fi
}

# Show how the current theme was resolved.
goh-theme() {
  local k
  for k in source osc adjusted surface-base surface-selected border-subtle text-primary text-muted accent; do
    [[ -n ${_goh_theme[$k]-} ]] && print -r -- "${(r:17:)k} ${_goh_theme[$k]}"
  done
}

# What is wrong when nothing shows? Prints each layer's state: run it at a prompt.
goh-status() {
  emulate -L zsh
  local ctl popup sock specs out
  ctl=$(command -v $_GOH_CTL) ; popup=$(command -v $_GOH_POPUP)
  sock=${GOH_SOCKET:-${XDG_RUNTIME_DIR:+$XDG_RUNTIME_DIR/goh/gohd.sock}}; sock=${sock:-/tmp/goh-$UID/gohd.sock}
  specs=${GOH_SPEC_DIR:-$HOME/.config/goh/specs}
  print -r -- "adapter    Tab is bound to: $(bindkey '^I' 2>/dev/null)   display mode: $_GOH_DISPLAY   as-you-type: $_GOH_LIVE"
  print -r -- "goh    ${ctl:-NOT FOUND on PATH (menus cannot work)}"
  print -r -- "gohd       socket $sock $([[ -S $sock ]] && print running || print 'not running (the first Tab starts it; press Tab again)')"
  if [[ -n $ctl ]]; then
    local -a spec_lines parts; local l summary=
    spec_lines=("${(@f)$(command $_GOH_CTL specs 2>/dev/null)}")
    for l in $spec_lines; do
      parts=("${(@ps:\t:)l}")
      if [[ ${parts[2]} == bundled ]]; then summary+="${summary:+, }${parts[1]} (bundled, ${parts[3]} subcommands)"
      else summary+="${summary:+, }${parts[1]} (yours: ${parts[2]#yours:}, ${parts[3]} subcommands${parts[4]:+; REPLACES the bundled one with ${parts[4]}})"; fi
    done
    print -r -- "specs      next daemon start; running daemon may differ: ${summary:-none}   [your own go in $specs; restart gohd after changes]"
  else
    print -r -- "specs      unknown (goh not found)"
  fi
  if [[ -n $ctl ]]; then
    out=$(command $_GOH_CTL menu --session $_GOH_SESSION --cwd $PWD --cursor-byte 5 --buffer 'git c' 2>/dev/null)
    local -a labels; local l
    for l in "${(@f)out}"; do [[ -n $l ]] && labels+=("${${(ps:\x1f:)l}[5]}"); done
    print -r -- "query      'git c' -> ${#labels} candidate(s)${labels:+: ${(j:, :)labels}}"
  fi
  print -r -- "theme      source ${_goh_theme[source]:-?}, osc ${_goh_theme[osc]:-?}, cell-height ${_goh_theme[cell-height]:-NONE (iTerm2 then needs it: the overlay will answer inline)}"
  print -r -- "popup      ${popup:-NOT FOUND on PATH (no overlay)}   running: $([[ -n $_goh_pop_pid ]] && _goh_pop_alive && print "yes (pid $_goh_pop_pid, state/age: $(command ps -o stat=,etime= -p $_goh_pop_pid 2>/dev/null))" || print no)   (state T = stopped)"
  print -r -- "last popup decision: ${_goh_last_decision:-none yet (open a menu with Tab first)}"
  print -r -- "last frame sent:     ${_goh_last_frame[1,260]:-none yet}"
  [[ -n $_goh_pop_dir && -f $_goh_pop_dir/err ]] && print -r -- "popup stderr: $_goh_pop_dir/err"
  if [[ -n $popup ]]; then
    print -r -- "gate       $(command $_GOH_POPUP --resolve 2>&1)"
  fi
}

# After an edit: open or update the menu for a valid line, or close it.
_goh_live_refresh() {
  emulate -L zsh
  setopt no_unset
  (( _goh_busy )) && return 0
  _goh_busy=1
  {
    local out was_on=$_goh_on cb
    if [[ $CONTEXT != start || -z ${BUFFER//[[:space:]]/} ]]; then _goh_close; return 0; fi
    _goh_bytes "${BUFFER[1,CURSOR]}"; cb=$REPLY
    out=$(command $_GOH_CTL menu --labels --live --late --session $_GOH_SESSION --cwd $PWD --limit 50 \
          --cursor-byte $cb --buffer "$BUFFER" 2>/dev/null) || out=
    _goh_late=0
    [[ ${out##*$'\n'} == pending ]] && _goh_late=1
    [[ -n $out ]] && _goh_parse_menu "$out" || REPLY=0
    if (( REPLY >= 1 )); then
      # Nothing to offer: too few characters typed, or a lone candidate that is just the finished word.
      # A finished word does not *open* a menu, but an open one is kept: closing it here only to reopen it
      # on the next space makes the popup blink at every word boundary.
      if (( cb - _goh_start[1] < _GOH_LIVE_MIN )); then REPLY=0
      elif (( REPLY == 1 && ! was_on )) && _goh_same_word "$BUFFER" ${_goh_start[1]} ${_goh_end[1]} "${_goh_insert[1]}"; then REPLY=0; fi
    fi
    if (( REPLY >= 1 )); then
      _goh_on=1 _goh_sel=1 _goh_win=1 _goh_buf=$BUFFER _goh_buf_cur=$CURSOR
      (( was_on )) || _goh_keys_on
      _goh_build_items
      [[ $_GOH_DISPLAY == inline ]] || _goh_pop_start
      _goh_render
      _goh_pop_show     # no zle -R: we are inside the redraw; the popup waits for the caret to settle
    else
      _goh_close
    fi
    (( _goh_late )) && _goh_late_wait
  } always {
    _goh_busy=0
  }
}

_goh_hook_redraw() {
  (( _goh_busy )) && return 0
  [[ -n $_goh_pred && -n $BUFFER ]] && _goh_drop_pred
  if (( _goh_reopen )); then
    _goh_reopen=0
    if (( ! PENDING )) && [[ $BUFFER == "$_goh_seen_buf" && $CURSOR == "$_goh_seen_cur" ]]; then
      _goh_live_refresh
      _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR
      return 0
    fi
  fi
  local changed=0
  [[ $BUFFER != "$_goh_seen_buf" || $CURSOR != $_goh_seen_cur ]] && changed=1
  _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR
  if [[ $_GOH_LIVE == on ]] && (( ${_goh_typing_widgets[(Ie)$LASTWIDGET]} )) && (( changed )); then
    if (( PENDING )); then _goh_trace "skip buf=[$BUFFER] pending=$PENDING"; return 0; fi   # refresh once the burst is read
    local t0=$EPOCHREALTIME ms
    _goh_live_refresh
    printf -v ms %.1f $(( (EPOCHREALTIME - t0) * 1000 ))
    _goh_trace "key buf=[$BUFFER] w=$LASTWIDGET open=$_goh_on cands=$(( _goh_on ? $#_goh_label : 0 )) top=${${_goh_label[1]}:#} ms=$ms"
    return 0
  fi
  (( _goh_on )) && ! _goh_valid && _goh_close       # the line changed some other way: the menu is stale
  return 0
}
_goh_hook_finish() { _goh_late_stop; _goh_late_tries=0 _goh_late_line=; _goh_close no-redisplay; _goh_drop_pred; _goh_seen_cur=-1; return 0 }

# Ctrl+C ends the line without zle-line-finish, so the zle-line-init hook is what closes the menu at the next
# prompt. A plugin that defines its own zle-line-init (or -finish, -pre-redraw) after this adapter loaded replaces
# the hook dispatcher, and then the popup would stay on screen until the next keystroke. Every prompt, before it
# is drawn, put the hooks back (add-zle-hook-widget keeps the plugin's widget as the first hook) and hide the popup
# of a menu that was left open.
typeset -gi _goh_update_told=0
typeset -ga _goh_zle_hooks=(zle-line-init:_goh_hook_finish zle-line-pre-redraw:_goh_hook_redraw zle-line-finish:_goh_hook_finish)
_goh_hook_precmd() {
  local h
  for h in $_goh_zle_hooks; do
    [[ ${widgets[${h%%:*}]-} == user:azhw:${h%%:*} ]] || add-zle-hook-widget ${h%%:*} ${h#*:}
  done
  (( _goh_on )) && _goh_pop_hide
  # `updates.check`: one line, at the first prompt only, when a newer release is known (see `goh update`).
  if (( ! _goh_update_told )); then
    _goh_update_told=1
    [[ ${GOH_UPDATE_CHECK-} == on ]] && command $_GOH_CTL update --notice 2>/dev/null
  fi
  return 0
}

# What the shell knows (aliases, functions, builtins, PATH, HISTFILE), so `g checkout ` completes as `git checkout ` and
# the command word completes from what is installed. Sent after a command changed any of it, never per keystroke.
# Each send is numbered: two of them can both wait for gohd to start, and gohd keeps the higher number.
typeset -g _goh_ctx_sig
typeset -gi _goh_ctx_seq
_goh_sync_context() {
  # Names of exported variables only (for `$NA` and `export NA`), never their values.
  local -a exported=(${(k)parameters[(R)*export*]})
  local sig="$PATH|${CDPATH-}|${HISTFILE-}|${(kv)aliases}|${(k)functions}|${(j: :)exported}"
  [[ $sig == "$_goh_ctx_sig" ]] && return 0
  _goh_ctx_sig=$sig
  (( ++_goh_ctx_seq ))
  {
    local k T=$'\t'   # a real tab: inside double quotes `\t` would stay two characters
    print -r -- "P$T$PATH"
    [[ -n ${CDPATH-} ]] && print -r -- "C$T$CDPATH"
    # Where this shell keeps its history; zsh does not export it, so gohd cannot see it otherwise.
    [[ ${HISTFILE-} == /* ]] && print -r -- "H$T$HISTFILE"
    for k in ${(k)aliases}; do [[ ${aliases[$k]} == *[$'\t\n']* ]] || print -r -- "A$T$k$T${aliases[$k]}"; done
    for k in ${(k)functions}; do [[ $k == [_.]* ]] || print -r -- "F$T$k"; done
    for k in ${(k)builtins}; do print -r -- "B$T$k"; done
    for k in $exported; do print -r -- "E$T$k"; done
  } | command $_GOH_CTL context --session $_GOH_SESSION --seq $_goh_ctx_seq >/dev/null 2>&1 &!
}

# Directory history for `cd` suggestions when zoxide is not installed (read by gohd; written here).
_goh_visit() { command $_GOH_CTL visit "$PWD" >/dev/null 2>&1 &! }

# Command record (privacy.recordCommands, GOH_RECORD=on; off by default and then nothing below is hooked in).
# The line and its folder are noted when it starts; a successful one is handed to `goh record`, which leaves out
# anything that looks secret and lines starting with a space.
typeset -g _goh_run_cmd _goh_run_cwd
_goh_run_start() { _goh_run_cmd=$1 _goh_run_cwd=$PWD }
# Ctrl+C ends the line without zle-line-finish, and zsh leaves what was drawn under the line on the screen. Before the
# next prompt is drawn the cursor is on the row after the menu, so moving up over the rows it took and clearing to the
# end of the screen takes it away. Only the menu's rows: the line above it stays as it was typed.
typeset -gi _goh_rows_shown=0
_goh_erase_left() {
  local st=$?
  (( _goh_rows_shown > 0 )) && print -nr -- $'\e['$_goh_rows_shown$'A\r\e[J'
  _goh_rows_shown=0
  return $st
}

_goh_run_done() {
  local st=$?   # first thing: the status of the command that just ended
  if (( st == 0 )) && [[ -n $_goh_run_cmd ]]; then
    { command $_GOH_CTL record --cwd $_goh_run_cwd -- "$_goh_run_cmd" >/dev/null 2>&1 &! } 2>/dev/null
  fi
  _goh_run_cmd= _goh_run_cwd=
  return $st
}

# At an empty prompt, the command that usually follows the last one run in this folder, as ghost text:
# Right or Tab takes it, typing anything else drops it. Off by default, so nothing shows until you type: needs
# GOH_PREDICT=on (display.predict) and the command record (GOH_RECORD=on).
typeset -g _goh_pred _goh_orig_right_csi _goh_orig_right_ss3
_goh_drop_pred() { [[ -n $_goh_pred ]] || return 0; _goh_pred=; _goh_clear_ghost }
_goh_hook_predict() {
  emulate -L zsh
  setopt no_unset
  _goh_pred=
  [[ ${GOH_RECORD-} == on && ${GOH_PREDICT-off} == on && $_GOH_LIVE == on && -z $BUFFER ]] || return 0
  (( _goh_busy )) && return 0
  local out
  out=$(command $_GOH_CTL menu --labels --live --session $_GOH_SESSION --cwd $PWD --limit 1 --cursor-byte 0 --buffer '' 2>/dev/null) || return 0
  [[ -n $out ]] || return 0
  local -a f
  f=("${(@ps:\x1f:)${out%%$'\n'*}}")
  [[ $f[4] == history && -n $f[6] ]] || return 0
  _goh_pred=$f[6]
  _goh_clear_ghost
  POSTDISPLAY=$_goh_pred
  is-at-least 5.9 && region_highlight+=("P0 ${#_goh_pred} $_goh_ghost_style memo=goh-ghost")
  return 0
}
_goh_accept_pred() {
  [[ -n $_goh_pred && -z $BUFFER ]] || return 1
  local line=$_goh_pred
  _goh_drop_pred
  BUFFER=$line
  CURSOR=${#BUFFER}
  _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR
}
_goh_right() {
  _goh_accept_pred && return 0
  local key=${(V)KEYS}
  if [[ $key == *'[C'* ]]; then zle ${_goh_orig_right_csi:-forward-char}; else zle ${_goh_orig_right_ss3:-forward-char}; fi
}

# `goh reload` restarts gohd; in this shell it also loads the adapter again, from the goh now installed and with the
# config read again, so no `exec $SHELL` is needed here. Other open shells keep theirs until they do the same.
goh() {
  # `goh config` runs $EDITOR: hand it over even when it is set in this shell but not exported.
  EDITOR=${EDITOR-} VISUAL=${VISUAL-} command $_GOH_CTL "$@" || return
  [[ ${1-} == reload && -n $_GOH_ORIG_TAB ]] || return 0   # after `goh-unload`, stays unloaded
  goh-unload
  (( $#_goh_from_config )) && unset $_goh_from_config   # values you set yourself stay
  eval "$(command $_GOH_CTL init zsh)" && print -r -- "goh: this shell's adapter is loaded again too"
}

goh-load() {
  local current=${${(z)$(bindkey '^I' 2>/dev/null)}[2]}
  [[ $current == _goh_tab ]] && return 0
  [[ -z $current || $current == undefined-key ]] && current=expand-or-complete
  _GOH_ORIG_TAB=$current
  zle -N _goh_tab
  zle -N _goh_up; zle -N _goh_down; zle -N _goh_esc
  zle -N _goh_enter; zle -N _goh_left
  zle -N _goh_on_late
  bindkey '^I' _goh_tab
  if [[ ${GOH_RECORD-} == on ]]; then
    # Right takes a prediction when one is showing and is the user's own Right otherwise.
    _goh_orig_right_csi=${${(z)$(bindkey '^[[C' 2>/dev/null)}[2]}; [[ $_goh_orig_right_csi == (undefined-key|_goh_right) ]] && _goh_orig_right_csi=forward-char
    _goh_orig_right_ss3=${${(z)$(bindkey '^[OC' 2>/dev/null)}[2]}; [[ $_goh_orig_right_ss3 == (undefined-key|_goh_right) ]] && _goh_orig_right_ss3=forward-char
    zle -N _goh_right
    bindkey '^[[C' _goh_right
    bindkey '^[OC' _goh_right
  fi
  # Ctrl+C aborts ZLE without line-finish. The next line-init must dismiss the old menu as well.
  add-zle-hook-widget zle-line-init _goh_hook_finish
  [[ ${GOH_RECORD-} == on ]] && add-zle-hook-widget zle-line-init _goh_hook_predict
  add-zle-hook-widget zle-line-pre-redraw _goh_hook_redraw
  add-zle-hook-widget zle-line-finish _goh_hook_finish
  autoload -Uz add-zsh-hook
  add-zsh-hook zshexit _goh_pop_stop
  add-zsh-hook chpwd _goh_visit
  add-zsh-hook precmd _goh_sync_context
  add-zsh-hook precmd _goh_hook_precmd
  if [[ ${GOH_RECORD-} == on ]]; then
    add-zsh-hook preexec _goh_run_start
    # Ahead of every other precmd: it needs the status of the command that just ended.
    add-zsh-hook precmd _goh_run_done
    precmd_functions=(_goh_run_done ${precmd_functions:#_goh_run_done})
  fi
  # First, before any other precmd can print under a menu left on the screen; it keeps the status for them.
  add-zsh-hook precmd _goh_erase_left
  precmd_functions=(_goh_erase_left ${precmd_functions:#_goh_erase_left})
  _goh_sync_context
  # `ssh` through `goh ssh` (popup.overSsh, on unless set off): a zsh host's menus are drawn by the popup here. Not
  # where no popup can draw them (over ssh, in a multiplexer, inline only), and never over your own `ssh`.
  if [[ ${GOH_POPUP_SSH:-on} == on && $_GOH_DISPLAY != inline && -z ${SSH_CONNECTION-}${SSH_TTY-}${TMUX-}${STY-} ]] &&
     (( ! ${+aliases[ssh]} && ! ${+functions[ssh]} )); then
    ssh() { command $_GOH_CTL ssh "$@" }
    _goh_ssh_wrapped=1
  fi
}

goh-unload() {
  _goh_late_stop 2>/dev/null
  _goh_close 2>/dev/null
  _goh_pop_stop 2>/dev/null
  [[ -n $_GOH_ORIG_TAB ]] && bindkey '^I' $_GOH_ORIG_TAB
  add-zle-hook-widget -d zle-line-init _goh_hook_finish
  add-zle-hook-widget -d zle-line-init _goh_hook_predict 2>/dev/null
  [[ -n $_goh_orig_right_csi ]] && { bindkey '^[[C' $_goh_orig_right_csi; bindkey '^[OC' ${_goh_orig_right_ss3:-forward-char} }
  add-zsh-hook -d preexec _goh_run_start 2>/dev/null
  add-zsh-hook -d precmd _goh_run_done 2>/dev/null
  add-zle-hook-widget -d zle-line-pre-redraw _goh_hook_redraw
  add-zle-hook-widget -d zle-line-finish _goh_hook_finish
  add-zsh-hook -d zshexit _goh_pop_stop 2>/dev/null
  add-zsh-hook -d chpwd _goh_visit 2>/dev/null
  add-zsh-hook -d precmd _goh_sync_context 2>/dev/null
  add-zsh-hook -d precmd _goh_hook_precmd 2>/dev/null
  add-zsh-hook -d precmd _goh_erase_left 2>/dev/null
  (( _goh_ssh_wrapped )) && { unfunction ssh 2>/dev/null; _goh_ssh_wrapped=0 }
  zle -D _goh_tab _goh_up _goh_down _goh_esc _goh_enter _goh_left _goh_right _goh_on_decision _goh_on_late 2>/dev/null
  unset _GOH_ORIG_TAB
}

goh-load
[[ -o interactive ]] && goh-theme-refresh
