# Adapters

Each adapter talks to `gohd` through `goh menu` and records directory changes with `goh visit`.

| Shell | File | How it completes | Status |
|---|---|---|---|
| zsh | `zsh/goh.zsh` | Own menu (overlay on iTerm2/Terminal.app, inline elsewhere), ghost text, as-you-type | tested |
| bash | `bash/goh.bash` | readline programmable completion | tested |
| fish | `fish/goh.fish` | fish `complete` (fish pager) | written, not run |
| PowerShell | `powershell/goh.ps1` | `Register-ArgumentCompleter -Native` | written, not run |
| nushell | `nushell/goh.nu` | external completer (you wire it in config.nu) | written, not run |
| elvish | `elvish/goh.elv` | `edit:completion:arg-completer` | written, not run |
| xonsh | `xonsh/goh.xsh` | xonsh completer API | written, not run |

Terminals: the shell adapters never depend on the terminal. In the integrated terminals of VS Code, JetBrains
and the Claude desktop app the zsh menu is drawn inline (the overlay needs a verified caret, which only iTerm2
and Terminal.app give); the other shells use their own completion UI there.
