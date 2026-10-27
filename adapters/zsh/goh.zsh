# goh.zsh — inline completion through gohd, on Tab and as you type. Source from ~/.zshrc after compinit.
#
# Tab      one candidate: insert it. Several: open an inline menu (ghost text + list below the line).
# Tab      with the menu open: insert the highlighted candidate (never runs the command).
# Up/Down  move the highlight, wrapping.   Shift+Tab: previous.   Esc: close.
# Enter    keeps the shell's default behaviour (the menu is dropped; nothing is inserted).
# With GOH_LIVE=off any edit or cursor move closes the menu (below: as-you-type keeps it in step instead).
# Up/Down/Shift+Tab/Esc are bound only while it is open.
#
# As-you-type (GOH_LIVE=on, the default; `off` = Tab only): after each edit, if the line is a recognised
# command in progress (its first word has a completion spec and every earlier word fits it; see
# docs/docs/18), the menu opens or updates by itself and follows what you type; when the line stops
# being valid or has no candidates it closes. Esc hides it until the line changes again. Unknown
# commands never open it by themselves (Tab still completes them). It reacts to typing and deleting
# only, not to history recall, and waits out bursts of input (paste, key repeat).
#
# Fail-open: any problem (no daemon, timeout, odd values) falls through to the widget Tab was
# bound to before. `goh-unload` restores the original binding.
# Overlay: with GOH_DISPLAY=auto (default) or overlay, the menu is also sent to `goh-popup`, which draws
# it at the caret when the caret is verified (iTerm2, Terminal.app) and otherwise answers "inline", in
# which case nothing changes. GOH_DISPLAY=inline never starts it. The inline menu is shown immediately
# and hidden only once the popup says it is drawing the overlay. Frames go over FIFOs; the answer is
# read asynchronously (zle -F), so a slow or dead popup can never block typing.
# Theme: ghost text uses the session's resolved `text-muted` (docs/design-system/theme-resolution.md);
# `goh-theme` shows how it was resolved, `goh-theme-refresh` asks the terminal again.
#
# Offsets on the wire are UTF-8 bytes; zsh's CURSOR and string slicing count characters, so
# conversions happen here with LC_ALL=C (byte semantics) and nowhere else.

autoload -Uz add-zle-hook-widget

# Sourcing again (e.g. after rebuilding) starts clean: stop a popup left by the previous version.
(( ${+functions[_goh_pop_stop]} )) && _goh_pop_stop 2>/dev/null

typeset -g _GOH_CTL=${GOH_CTL:-goh-ctl}
typeset -g _GOH_SESSION="zsh-$$-$RANDOM"
typeset -g _GOH_LIMIT=${GOH_LIMIT:-8}
typeset -g _GOH_DISPLAY=${GOH_DISPLAY:-auto}   # auto | overlay | inline
typeset -g _GOH_LIVE=${GOH_LIVE:-on}           # on | off (as-you-type suggestions)
typeset -gi _GOH_LIVE_MIN=${GOH_LIVE_MIN:-0}   # characters of the current word before the menu may open by itself
typeset -g _goh_seen_buf
typeset -gi _goh_seen_cur=-1 _goh_busy=0
# Widgets after which an edit counts as the user typing (not history recall, autosuggest accept, ...).
typeset -ga _goh_typing_widgets=(self-insert backward-delete-char delete-char vi-backward-delete-char vi-delete-char backward-kill-word kill-word bracketed-paste quoted-insert)
typeset -g _GOH_POPUP=${GOH_POPUP:-goh-popup}
typeset -g _GOH_ORIG_TAB
typeset -g _goh_pop_dir _goh_pop_pid _goh_items_json _goh_last_decision _goh_last_frame
typeset -gi _goh_pin=-1 _goh_pout=-1 _goh_overlay=0 _goh_win=1
typeset -ga _goh_ranges
typeset -g _goh_new _goh_pre _goh_tail_out
typeset -gi _goh_on=0 _goh_sel=1 _goh_buf_cur=0
typeset -g _goh_buf _goh_hl
typeset -ga _goh_start _goh_end _goh_cursor _goh_kind _goh_label _goh_insert _goh_desc
typeset -gA _goh_theme   # resolved design tokens (see goh-theme-refresh)
typeset -g _goh_ghost_style='fg=8'
typeset -gA _goh_saved   # key -> widget it was bound to before the menu opened ("" = unbound)

# Menu-only keys and the widget each gets.
typeset -gA _goh_menu_keys=(
  '^[[A' _goh_up   '^[OA' _goh_up
  '^[[B' _goh_down '^[OB' _goh_down
  '^[[Z' _goh_up
  '^['   _goh_esc
)

# GOH_TRACE=/path/file logs what the adapter does on each keystroke (time, line, menu state, cost), to
# see how it feels without watching a screen. Off unless set.
zmodload -s zsh/datetime
_goh_trace() { [[ -n ${GOH_TRACE-} ]] && print -r -- "$EPOCHREALTIME $*" >> $GOH_TRACE; return 0 }

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
  local line
  if [[ -n ${2-} ]]; then _goh_pop_stop; return 0; fi    # hup/nval: the popup went away
  while read -r -u $1 -t 0 line; do
    _goh_last_decision=$line
    _goh_trace "decision ${line[1,80]}"
    case $line in
      *'"display":"overlay"'*) _goh_overlay=1 ;;
      *'"display":"inline"'*)  _goh_overlay=0 ;;
    esac
  done
  _goh_valid && _goh_render                              # show or clear the inline list to match
  return 0
}

# Start the popup on first use. Returns non-zero (and leaves everything inline) on any problem.
_goh_pop_start() {
  _goh_pop_alive && return 0
  _goh_pop_stop
  [[ $_GOH_DISPLAY == inline ]] && return 1
  command -v $_GOH_POPUP >/dev/null 2>&1 || return 1
  local dir
  dir=$(mktemp -d "${TMPDIR:-/tmp}/goh-popup.XXXXXX" 2>/dev/null) || return 1
  mkfifo $dir/in $dir/out 2>/dev/null || { rm -rf -- $dir; return 1 }
  local errf=/dev/null; [[ -n ${GOH_DEBUG-} ]] && errf=$dir/err   # GOH_DEBUG=1 keeps the popup's stderr
  command $_GOH_POPUP <$dir/in >$dir/out 2>$errf &!
  _goh_pop_pid=$!
  _goh_pop_dir=$dir
  # Read-write opens never block, and keep a reader on `in` so a dead popup cannot raise SIGPIPE.
  if ! { exec {_goh_pin}<>$dir/in {_goh_pout}<>$dir/out } 2>/dev/null; then _goh_pop_stop; return 1; fi
  zle -F $_goh_pout _goh_on_decision
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
  local n=$#_goh_label rows
  (( rows = n < _GOH_LIMIT ? n : _GOH_LIMIT ))
  # The window moves one row at a time and jumps only when the selection wraps.
  if (( _goh_sel < _goh_win )); then _goh_win=$_goh_sel
  elif (( _goh_sel >= _goh_win + rows )); then _goh_win=$(( _goh_sel + 1 - rows )); fi
  local theme= k ctx cell=
  for k in surface-base surface-selected border-subtle text-primary text-muted accent text-match; do
    [[ ${_goh_theme[$k]-} =~ '^#[0-9a-f]{6}$' ]] && theme+="${theme:+,}\"$k\":\"${_goh_theme[$k]}\""
  done
  [[ ${_goh_theme[cell-height]-} =~ '^[0-9]+(\.[0-9]+)?$' ]] && cell=",\"cell_height\":${_goh_theme[cell-height]}"
  _goh_json "${${(z)BUFFER}[1]}"; ctx=$REPLY
  _goh_trace "frame sel=$_goh_sel n=$#_goh_label"
  _goh_last_frame="{\"type\":\"show\",\"state\":{\"id\":\"goh\",\"items\":$_goh_items_json,\"selected\":$(( _goh_sel - 1 )),\"window_start\":$(( _goh_win - 1 )),\"context\":\"$ctx\"},\"anchor\":\"auto\",\"mode\":\"$_GOH_DISPLAY\",\"max_rows\":$_GOH_LIMIT,\"theme\":{$theme}$cell}"
  print -r -u $_goh_pin -- "$_goh_last_frame" 2>/dev/null
}

_goh_pop_hide() {
  _goh_trace hide
  _goh_pop_alive && (( _goh_pin >= 0 )) && print -r -u $_goh_pin -- '{"type":"hide"}' 2>/dev/null
}

# ---- menu state ---------------------------------------------------------------------------

_goh_keys_on() {
  local k
  for k in ${(k)_goh_menu_keys}; do
    _goh_saved[$k]=${${(z)$(bindkey -- $k 2>/dev/null)}[2]}
    [[ ${_goh_saved[$k]} == undefined-key ]] && _goh_saved[$k]=
    bindkey -- $k ${_goh_menu_keys[$k]}
  done
}

_goh_keys_off() {
  local k
  for k in ${(k)_goh_menu_keys}; do
    if [[ -n ${_goh_saved[$k]-} ]]; then bindkey -- $k ${_goh_saved[$k]}; else bindkey -r -- $k; fi
  done
  _goh_saved=()
}

_goh_close() {
  (( _goh_on )) || return 0
  _goh_on=0
  POSTDISPLAY=
  [[ -n $_goh_hl ]] && region_highlight=("${(@)region_highlight:#$_goh_hl}")
  _goh_hl=
  zle -M ''
  _goh_pop_hide
  _goh_keys_off
}

# Menu is only valid while the line is exactly as it was when it opened.
_goh_valid() { (( _goh_on )) && [[ $BUFFER == "$_goh_buf" && $CURSOR == $_goh_buf_cur ]] }

_goh_render() {
  local i marker line room desc
  local -a rows
  local width=$(( ${COLUMNS:-80} - 1 ))
  for (( i = 1; i <= $#_goh_label; i++ )); do
    marker=' '; (( i == _goh_sel )) && marker='❯'
    line="$marker ${_goh_label[i]}"
    desc=${_goh_desc[i]:-${_goh_kind[i]}}   # kind keeps rows meaningful without icons
    room=$(( width - ${#line} - 2 ))
    if (( room > 3 )); then
      (( ${#desc} > room )) && desc="${desc[1,room-1]}…"
      line+="  $desc"
    fi
    rows+=("${line[1,width]}")
  done
  rows+=("  Tab insert · ↑↓ move · Esc close")
  if (( _goh_overlay )) && _goh_pop_alive; then zle -M ''; else _goh_overlay=0; zle -M "${(F)rows}"; fi

  # Ghost text only where it is honest: cursor at the end and the insertion extends what is typed.
  [[ -n $_goh_hl ]] && region_highlight=("${(@)region_highlight:#$_goh_hl}")
  _goh_hl=; POSTDISPLAY=
  _goh_bytes "$BUFFER"
  if (( _goh_end[_goh_sel] == REPLY && CURSOR == ${#BUFFER} )); then
    _goh_tail "$BUFFER" ${_goh_start[_goh_sel]} "${_goh_insert[_goh_sel]}"
    if [[ -n $_goh_tail_out ]]; then
      POSTDISPLAY=$_goh_tail_out
      # "P" offsets count from the start of PREDISPLAY+BUFFER+POSTDISPLAY.
      local from=$(( ${#PREDISPLAY} + ${#BUFFER} ))
      _goh_hl="P$from $(( from + ${#POSTDISPLAY} )) $_goh_ghost_style"
      region_highlight+=("$_goh_hl")
    fi
  fi
}

# Apply candidate $1 to the line; fails (leaving the line alone) on any inconsistency.
_goh_apply() {
  local i=$1 buflen
  _goh_bytes "$BUFFER"; buflen=$REPLY
  (( _goh_start[i] <= _goh_end[i] && _goh_end[i] <= buflen && _goh_cursor[i] >= _goh_start[i] )) || return 1
  [[ -n ${_goh_insert[i]} ]] || return 1
  _goh_splice "$BUFFER" ${_goh_start[i]} ${_goh_end[i]} "${_goh_insert[i]}" ${_goh_cursor[i]}
  BUFFER=$_goh_new
  CURSOR=${#_goh_pre}
}

# ---- widgets ------------------------------------------------------------------------------

_goh_move() {  # _goh_move DELTA ORIG_WIDGET
  if _goh_valid; then
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
_goh_esc()  { _goh_close; zle -R }   # stays closed: the hook refreshes only when the line changes

# Parse `goh-ctl menu` output ($1) into the _goh_* arrays; REPLY = number of candidates.
_goh_parse_menu() {
  local line; local -a f
  _goh_start=() _goh_end=() _goh_cursor=() _goh_kind=() _goh_label=() _goh_insert=() _goh_desc=() _goh_ranges=()
  for line in "${(@f)1}"; do
    f=("${(@ps:\x1f:)line}")
    [[ $f[1] == <-> && $f[2] == <-> && $f[3] == <-> ]] || continue
    _goh_start+=($f[1]) _goh_end+=($f[2]) _goh_cursor+=($f[3])
    _goh_kind+=("$f[4]") _goh_label+=("$f[5]") _goh_insert+=("$f[6]") _goh_desc+=("$f[7]") _goh_ranges+=("$f[8]")
  done
  REPLY=$#_goh_label
}

_goh_tab() {
  emulate -L zsh
  setopt no_unset

  if _goh_valid; then
    _goh_apply $_goh_sel; _goh_close
    _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR   # the next keystroke, not the accept, reopens it
    return 0
  fi
  _goh_close

  if [[ $CONTEXT == start && -n ${BUFFER//[[:space:]]/} ]]; then
    _goh_bytes "${BUFFER[1,CURSOR]}"
    local out
    out=$(command $_GOH_CTL menu --session $_GOH_SESSION --cwd $PWD --limit $_GOH_LIMIT \
          --cursor-byte $REPLY --buffer "$BUFFER" 2>/dev/null) || out=
    if [[ -n $out ]]; then
      _goh_parse_menu "$out"
      if (( $#_goh_label == 1 )); then
        _goh_apply 1 && { _goh_seen_buf=$BUFFER _goh_seen_cur=$CURSOR; return 0 }
      elif (( $#_goh_label > 1 )); then
        _goh_on=1 _goh_sel=1 _goh_win=1 _goh_buf=$BUFFER _goh_buf_cur=$CURSOR
        _goh_keys_on
        _goh_build_items
        _goh_render
        zle -R
        _goh_pop_show
        return 0
      fi
    fi
  fi
  zle ${_GOH_ORIG_TAB:-expand-or-complete}
}

# Resolve the theme for this session. With a terminal attached it asks for the default colours
# (OSC 10/11) via goh-ctl, which owns the bounded, filtered tty read. Run between prompts, never
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
  print -r -- "goh-ctl    ${ctl:-NOT FOUND on PATH (menus cannot work)}"
  print -r -- "gohd       socket $sock $([[ -S $sock ]] && print running || print 'not running (the first Tab starts it; press Tab again)')"
  print -r -- "specs      $specs: ${(j:, :)${(@)${(f)$(command ls $specs 2>/dev/null)}:-NONE (no completions: copy docs/examples/git.completion.json to $specs/git.json)}}"
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
    out=$(command $_GOH_CTL menu --live --session $_GOH_SESSION --cwd $PWD --limit $_GOH_LIMIT \
          --cursor-byte $cb --buffer "$BUFFER" 2>/dev/null) || out=
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
      _goh_render
      _goh_pop_show     # no zle -R: we are inside the redraw; the popup waits for the caret to settle
    else
      _goh_close
    fi
  } always {
    _goh_busy=0
  }
}

_goh_hook_redraw() {
  (( _goh_busy )) && return 0
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
_goh_hook_finish() { _goh_close; _goh_seen_cur=-1; return 0 }

goh-load() {
  local current=${${(z)$(bindkey '^I' 2>/dev/null)}[2]}
  [[ $current == _goh_tab ]] && return 0
  [[ -z $current || $current == undefined-key ]] && current=expand-or-complete
  _GOH_ORIG_TAB=$current
  zle -N _goh_tab
  zle -N _goh_up; zle -N _goh_down; zle -N _goh_esc
  bindkey '^I' _goh_tab
  add-zle-hook-widget zle-line-pre-redraw _goh_hook_redraw
  add-zle-hook-widget zle-line-finish _goh_hook_finish
  autoload -Uz add-zsh-hook
  add-zsh-hook zshexit _goh_pop_stop
}

goh-unload() {
  _goh_close 2>/dev/null
  _goh_pop_stop 2>/dev/null
  [[ -n $_GOH_ORIG_TAB ]] && bindkey '^I' $_GOH_ORIG_TAB
  add-zle-hook-widget -d zle-line-pre-redraw _goh_hook_redraw
  add-zle-hook-widget -d zle-line-finish _goh_hook_finish
  add-zsh-hook -d zshexit _goh_pop_stop 2>/dev/null
  zle -D _goh_tab _goh_up _goh_down _goh_esc 2>/dev/null
  unset _GOH_ORIG_TAB
}

goh-load
[[ -o interactive ]] && goh-theme-refresh
