# docs/media/features.gif, the README's first picture. Run scripts/record-demo.sh from any
# directory: it creates this shell, a real disposable Git repo and project, and a private
# daemon, and cleans up even on failure. Everything shown is goh's own output.
Set Shell zsh
Set FontFamily "Menlo"
Set FontSize 22
Set Width 1100
Set Height 440
Set Padding 24
Set Framerate 25
Set CursorBlink false
Set TypingSpeed 65ms
Set Theme "Catppuccin Mocha"

Hide
Wait+Screen /project ❯/
Sleep 300ms
Show
Sleep 600ms

# Live filtering, keyboard navigation, then Tab inserts the selected command.
Type "git chec"
Wait+Screen /❯.*check-ignore/
Sleep 700ms
Down
Wait+Screen /❯.*checkout/
Sleep 600ms
Tab
Sleep 350ms

# These are branches in the fixture's real Git repository, read by goh.
Type "feature/"
Wait+Screen /feature\/auth/
Sleep 500ms
Down
Sleep 400ms
Tab
Sleep 400ms
Enter
Wait+Screen /Switched to branch 'feature\/search'/
Sleep 500ms
Ctrl+L
Sleep 200ms

# Real bundled Docker spec; Docker itself need not be installed or running.
Type "docker run --"
Wait+Screen /❯.*--add-host/
Sleep 500ms
Down
Wait+Screen /❯.*--attach/
Sleep 600ms
Tab
Sleep 400ms
Ctrl+U
Sleep 200ms

# The warning belongs to the suggestion. Never submit this line.
Type "git push --force"
Wait+Screen /overwrites the remote branch/
Sleep 1300ms
Ctrl+U
Sleep 200ms

# Fuzzy matching corrects the typo; Tab inserts the actual result.
Type "git chekcout"
Wait+Screen /↳ checkout/
Sleep 900ms
Tab
Sleep 500ms
Ctrl+U
Sleep 200ms

# Folders open as a tree: → shows a folder's entries under it, Tab inserts the whole path.
Type "ls src/"
Wait+Screen /components\//
Sleep 500ms
Right
Wait+Screen /Button\.tsx/
Sleep 500ms
Down
Sleep 400ms
Tab
Wait+Screen /src\/components\/Header\.tsx/
Sleep 700ms

# Back to the empty prompt the recording starts on, so the loop has no seam.
Ctrl+U
Ctrl+L
Sleep 700ms
