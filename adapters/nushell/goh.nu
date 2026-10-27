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
# Fail-open: returns null (nushell falls back to its own completion) when goh has nothing.

export def goh-complete [spans: list<string>] {
    let buffer = ($spans | str join ' ')
    let point = ($buffer | str length --bytes)
    let cur_bytes = ($spans | last | str length --bytes)
    let sep = (char unit_separator)
    let rows = (do { ^goh menu --session $"nu-($nu.pid)" --cwd $env.PWD --limit 50 --cursor-byte ($point | into string) --buffer $buffer } | complete)
    if $rows.exit_code != 0 { return null }
    $rows.stdout | lines | each {|line|
        let f = ($line | split row $sep)
        if ($f.0 | into int) == ($point - $cur_bytes) and ($f.1 | into int) == $point {
            { value: ($f.5 | str trim --right | str replace --all '\ ' ' '), description: ($f.6? | default '') }
        }
    } | compact
}
