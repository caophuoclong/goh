# docs/media/demo.gif, the README's second picture: what goh reads from the project and,
# opted in, from your history. Run scripts/record-demo.sh (see demo.tape); this shell has
# GOH_RECORD=on and a fixture zsh history, so the history lines below are goh's real output.
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

# package.json scripts, each with the command it runs.
Type "npm run "
Wait+Screen /vite build/
Sleep 900ms
Down
Sleep 400ms
Tab
Sleep 500ms
Ctrl+U
Sleep 200ms

# Makefile targets.
Type "make "
Wait+Screen /release/
Sleep 900ms
Ctrl+U
Sleep 200ms

# Your own snippet: Tab expands it and puts the cursor inside the quotes.
Type "gcm"
Wait+Screen /Commit with a message/
Sleep 700ms
Tab
Sleep 300ms
Type "Add search"
Sleep 700ms
Ctrl+U
Sleep 200ms

# History: goh shows the frequent line as ghost text and as a selectable row.
Type "docker comp"
Wait+Screen /docker compose up -d/
Sleep 1100ms
Down
Wait+Screen /❯ ↺ docker compose up -d/
Sleep 350ms
Tab
Sleep 800ms
Ctrl+U
Sleep 200ms

# A secret in a history line is masked in the menu and the ghost text. Never take it here.
Type "export API"
Wait+Screen /API_TOKEN=\*\*\*\*/
Sleep 1600ms

# Back to the empty prompt the recording starts on, so the loop has no seam.
Ctrl+U
Ctrl+L
Sleep 700ms
