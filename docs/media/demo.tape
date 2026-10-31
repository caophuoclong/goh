# Re-record after a UI change: vhs docs/media/demo.tape (from the repository root, after a release build).
# The recording runs its own daemon on a private socket and ignores your config. Stop it afterwards with
# GOH_SOCKET=${TMPDIR:-/tmp}/goh-demo/s target/release/goh stop
Output docs/media/demo.gif
Set Shell zsh
Set FontSize 18
Set Width 960
Set Height 480
Set Padding 16
Set TypingSpeed 90ms
Set Theme "Catppuccin Mocha"

Hide
Type "export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8"
Enter
Type "d=${TMPDIR:-/tmp}/goh-demo; mkdir -m 700 -p $d && export GOH_SOCKET=$d/s GOH_CONFIG=/nonexistent GOH_USAGE=$d/usage GOH_VISITS=$d/dirs GOH_RUNS=$d/runs GOH_LOG=$d/log GOH_UPDATE_STATE=$d/update"
Enter
Type "export GOH_CTL=$PWD/target/release/goh GOH_DISPLAY=inline GOH_LIVE=on PS1='%F{blue}~/project%f ❯ '"
Enter
Type "source adapters/zsh/goh.zsh && clear"
Enter
Show

Sleep 600ms
Type "git chec"
Sleep 1s
Tab
Sleep 400ms
Type "-"
Sleep 1s
Ctrl+U
Sleep 400ms

Type "docker run --"
Sleep 1s
Down
Down
Sleep 800ms
Escape
Ctrl+U
Sleep 400ms

Type "git push --fo"
Sleep 1500ms
Escape
Ctrl+U
Sleep 400ms

Type "git chekcout"
Sleep 1500ms
Escape
Ctrl+U
Sleep 600ms
