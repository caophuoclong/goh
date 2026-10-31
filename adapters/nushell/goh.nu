# goh.nu — spec-driven completion for nushell through its external completer.
#
# Nushell takes ONE external completer, so goh cannot install itself. Add to config.nu:
#
#   source /path/to/goh.nu
#   $env.config.completions.external = { enable: true, completer: {|spans| goh-complete $spans } }
#
# Records the working directory on every change (for `cd` suggestions without zoxide):
#
#   $env.config.hooks.env_change.PWD = ($env.config.hooks.env_change.PWD? | default [] | append {|before, after| ^goh visit $after })
#
# A box as you type: after `source`, run `goh-ui` in config.nu. Space then opens a bordered menu of goh's candidates
# for the next word, which filters as you keep typing; Down/Up move, Enter takes one, Esc closes it.
#
# Fail-open: returns null (nushell falls back to its own completion) when goh has nothing.

export def goh-complete [spans: list<string>] {
    try {
        let buffer = ($spans | str join ' ')
        let point = ($buffer | encode utf8 | bytes length)
        let cur_bytes = ($spans | last | encode utf8 | bytes length)
        let sep = (char unit_separator)
        let rows = (do { ^goh menu --session $"nu-($nu.pid)" --cwd $env.PWD --limit 50 --cursor-byte ($point | into string) --buffer $buffer } | complete)
        if $rows.exit_code != 0 { return null }
        $rows.stdout | lines | each {|line|
            let f = ($line | split row $sep)
            if ($f.0 | into int) == ($point - $cur_bytes) and ($f.1 | into int) == $point {
                { value: ($f.5 | str trim --right | str replace --all '\ ' ' '), description: ($f.6? | default '') }
            }
        } | compact
    } catch { null }
}

# Candidates for the box: the line and the cursor (a byte offset) as nushell gives them to a menu's source.
# Fails open: a missing goh (nushell 0.116 throws on a missing external) or a malformed row gives no candidates.
export def goh-menu-source [buffer: string, position: int] {
    try {
        let sep = (char unit_separator)
        let rows = (do { ^goh menu --live --session $"nu-($nu.pid)" --cwd $env.PWD --limit 50 --cursor-byte ($position | into string) --buffer $buffer } | complete)
        if $rows.exit_code != 0 { return [] }
        $rows.stdout | lines | each {|line|
            let f = ($line | split row $sep)
            if ($f | length) >= 7 {
                {
                    value: ($f.5 | str trim --right | str replace --all '\ ' ' ')
                    description: ($f.6? | default '')
                    span: { start: ($f.0 | into int), end: ($f.1 | into int) }
                }
            }
        } | compact
    } catch { [] }
}

# Add goh's box: menu `goh_menu` and a Space binding that opens it. Your own menus and bindings stay.
export def --env goh-ui [] {
    if ('persistent_menus' in ($env.config.completions | columns)) {
        $env.config.completions.persistent_menus = true
    }
    $env.config.menus = ($env.config.menus | append {
        name: goh_menu
        only_buffer_difference: false
        marker: ""
        type: {
            layout: ide
            min_completion_width: 0
            max_completion_width: 40
            max_completion_height: 10
            padding: 0
            border: true
            cursor_offset: 0
            description_mode: "prefer_right"
            min_description_width: 0
            max_description_width: 50
            max_description_height: 10
            description_offset: 1
            correct_cursor_pos: true
        }
        style: {
            text: default
            selected_text: { attr: r }
            description_text: dark_gray
            match_text: { attr: u }
            selected_match_text: { attr: ur }
        }
        source: {|buffer, position| goh-menu-source $buffer $position }
    })
    $env.config.keybindings = ($env.config.keybindings | append {
        name: goh_space
        modifier: none
        keycode: space
        mode: [emacs vi_insert]
        event: [
            { edit: insertstring value: " " }
            { send: menu name: goh_menu }
        ]
    })
}
