# Windows checklist: install to tested

Everything goh does on Windows, in the order to check it, from a clean machine to every feature tested. Written to be
followed by a person or by an agent driving the Windows machine: each step names the command, what must happen, and
what to report when it does not.

Mark each box as it passes. A failure is reported with the step number and the **Report** items of that step.

What exists on which branch:

| Part | State |
|---|---|
| Popup in Windows Terminal (`goh-popup.exe`, caret through UI Automation) | on `feat/windows-popup` |
| WSL: zsh in a Windows Terminal pane uses that popup | on `feat/windows-popup` |
| Test kit built by CI (`goh-windows-test-kit`): install without building | on `feat/windows-popup` |
| PowerShell native (daemon and `goh` on Windows, menu in PowerShell) | not built yet: section 6 is skipped |
| Windows installer (`install.ps1`), one command installing both sides | not built yet: section 7 is skipped |

## 0. Machine

- [ ] 0.1 Windows 10 21H2+ or Windows 11. `winver` shows the version.
- [ ] 0.2 Windows Terminal 1.18+ from the Store (or winget `Microsoft.WindowsTerminal`). In it: Settings > About.
- [ ] 0.3 WSL 2 with a distro (Ubuntu): `wsl -l -v` in PowerShell shows it with VERSION 2. If not: `wsl --install -d Ubuntu`, then reboot.
- [ ] 0.4 WSL interop is on: in the distro, `cmd.exe /c ver` prints the Windows version. If it fails, `/etc/wsl.conf` has
      `[interop] enabled=false`: remove that and run `wsl --shutdown` in PowerShell.
- [ ] 0.5 WSL sees the Windows PATH: in the distro, `command -v cmd.exe` prints `/mnt/c/...`. If not, `/etc/wsl.conf`
      has `appendWindowsPath=false`: remove it, `wsl --shutdown`.
- [ ] 0.6 zsh in the distro is the login shell: `sudo apt update && sudo apt install -y zsh`, then `chsh -s "$(command -v zsh)"`;
      a new Ubuntu tab runs zsh (answer `0` to its first-run questions) and `echo $SHELL` ends in `zsh`.
- [ ] 0.7 Display scaling noted (Settings > System > Display > Scale) for each monitor: steps 5.16 and 5.17 need it.

## 1. Get the test kit (built by CI, nothing to compile)

Every push to the pull request of `feat/windows-popup` runs CI, whose `Windows test kit` job uploads the artifact
`goh-windows-test-kit`: `goh-popup.exe`, the Linux package for WSL with its `.sha256`, `install.sh`,
`install-test-kit.ps1`, `overlay-demo.ps1` with its sample frame, this checklist, and `COMMIT` (the commit it was built
from). Downloading an artifact needs a GitHub sign-in.

- [ ] 1.1 With the GitHub CLI (`winget install --id GitHub.cli -e`, new terminal, `gh auth login`):
      ```powershell
      $run = gh run list -R caophuoclong/goh --branch feat/windows-popup --workflow ci.yml --limit 1 --json databaseId --jq '.[0].databaseId'
      gh run watch $run -R caophuoclong/goh   # waits while the newest run is still going
      Remove-Item -Recurse -Force $HOME\goh-kit -ErrorAction SilentlyContinue
      gh run download $run -R caophuoclong/goh -n goh-windows-test-kit -D $HOME\goh-kit
      ```
      `gh run download` says `no artifact matches` when that run's `Windows test kit` job did not pass: report it.
      Or in a browser: the pull request > Checks > CI > Summary > Artifacts > `goh-windows-test-kit`, then unpack the zip
      to `$HOME\goh-kit`.
- [ ] 1.2 `Get-Content $HOME\goh-kit\COMMIT` is the commit you mean to test (the branch's newest:
      `gh api repos/caophuoclong/goh/commits/feat/windows-popup --jq .sha`).

## 2. Install from the kit

- [ ] 2.1 PowerShell: `powershell -ExecutionPolicy Bypass -File $HOME\goh-kit\install-test-kit.ps1` (add `-Distro <name>`
      when the distro is not the default one). It ends with `goh: done.` and prints, from WSL, `installed goh-dev-…` and the
      goh block added to `~/.zshrc`. A checksum mismatch stops it with `does not match`.
- [ ] 2.2 Close every Windows Terminal window (WSL takes PATH from the Windows side when it starts), then `wsl --shutdown`.
- [ ] 2.3 New Windows Terminal, PowerShell tab: `Get-Command goh-popup.exe` shows
      `%LOCALAPPDATA%\Programs\goh\goh-popup.exe`.
- [ ] 2.4 New Ubuntu tab: `command -v goh-popup.exe` prints `/mnt/c/Users/<you>/AppData/Local/Programs/goh/goh-popup.exe`,
      and `grep -c '>>> goh >>>' ~/.zshrc` prints `1`.
- [ ] 2.5 `echo $WT_SESSION` prints a GUID (Windows Terminal passes it into WSL). Empty means the tab is not in
      Windows Terminal (or the old console host): steps in section 5 then stay inline by design.
- [ ] 2.6 Running `install-test-kit.ps1` a second time succeeds, keeps one goh block in `~/.zshrc` and one goh entry in
      the user PATH.

**Report** for any failure here: the full output of the failing command.

<details>
<summary>Without the kit: build from source (slow)</summary>

On Windows: Git (`winget install --id Git.Git -e`), Visual Studio Build Tools with the C++ workload
(`winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`),
Rust (`winget install --id Rustlang.Rustup -e`), then
`git clone https://github.com/caophuoclong/goh.git $HOME\goh; cd $HOME\goh; git switch feat/windows-popup; cargo build --release --locked -p goh-popup`
and copy `target\release\goh-popup.exe` to `%LOCALAPPDATA%\Programs\goh`, on the user PATH.
In WSL: `sudo apt install -y build-essential curl git`, Rust from https://rustup.rs, then
`git clone https://github.com/caophuoclong/goh.git ~/goh && cd ~/goh && git switch feat/windows-popup && ./install.sh --from-source`.
Then continue at 2.2.

</details>

## 3. Unit tests on Windows

- [ ] 3.1 CI's `Windows popup (test, release build)` job passed for the kit's commit (the pull request's Checks list
      it). Locally, from a source checkout: `cargo test --locked -p goh-anchor -p goh-popup -p goh-theme`.

## 4. The popup alone, from PowerShell (checks the caret reading without WSL)

Run in a PowerShell tab of Windows Terminal, window not maximised, on the main monitor, in `$HOME\goh-kit`:

- [ ] 4.1 `goh-popup.exe --resolve` prints JSON with `"display":"overlay"`, `"app":"Windows Terminal"`, a `caret` and
      `display` rectangle, and `scale` equal to the monitor's scale (1.5 for 150 %).
- [ ] 4.2 `powershell -ExecutionPolicy Bypass -File .\overlay-demo.ps1`: a dark list of git subcommands appears for
      about 6 s **directly under** the `PS> git c` line, its left edge at the cursor. The decision printed at the end
      says `"display":"overlay"`.
- [ ] 4.3 While 4.2 runs, the Windows Terminal window keeps its active title bar (the popup took no focus), and the
      popup has no taskbar button and does not show in Alt+Tab.
- [ ] 4.4 Clicking on the popup while 4.2 runs reaches the terminal under it (the popup lets clicks through).
- [ ] 4.5 Run 4.2 with the prompt on the last rows of the window (press Enter until the prompt is at the bottom):
      the popup opens **above** the line instead.
- [ ] 4.6 In another terminal app (the old console: run `conhost.exe powershell` from Win+R), `goh-popup.exe --resolve`
      says `"display":"inline"` with the reason that the shell is not running in Windows Terminal.

**Report**: the `--resolve` JSON (it includes a `trace`), the decision line from 4.2, and a screenshot of the
screen while the popup is up.

## 5. WSL: zsh menus drawn by the popup

All in an Ubuntu tab of Windows Terminal with zsh and goh loaded (section 2). Type the commands; do not paste them.

- [ ] 5.1 `goh doctor` lists `goh-popup.exe` as found under `binary` and says
      `Windows Terminal (WSL): the popup is drawn by goh-popup.exe on Windows` under `terminal`.
- [ ] 5.2 `goh-status` ends with a `gate` line whose JSON says `"display":"overlay"`.
- [ ] 5.3 Type `git ` then press Tab: the menu shows as a popup under the cursor, not drawn in the terminal's text.
      The first menu may take about a second (the Windows program starts); later ones are immediate.
- [ ] 5.4 ↑ and ↓ move the highlight in the popup; Tab takes the highlighted item into the line; Esc closes the popup.
- [ ] 5.5 Keep typing letters with the popup open: each key goes to the shell (the popup never takes the keyboard),
      and the list narrows as you type.
- [ ] 5.6 As-you-type: type `git ch` slowly; the popup opens by itself (unless `display.live` is off) and follows the
      word.
- [ ] 5.7 Rows show icons (git logo in the footer, folder and file icons for `ls ` + Tab) and colours that follow the
      terminal's colour scheme.
- [ ] 5.8 Font: the popup uses the profile's font face and size from Windows Terminal (Settings > Profiles > Ubuntu >
      Appearance). Change the size, open a new tab, press Tab: the popup's text size follows.
- [ ] 5.9 `cd ` + Tab on a folder, then →: the folder's entries show under it with tree lines; ← goes back.
- [ ] 5.10 Risky line: `rm -rf ` + Tab shows its warning in red.
- [ ] 5.11 Window changes with a menu open: move the Windows Terminal window, the popup follows; minimise it, the
      popup hides; restore it, the popup comes back; click another app, the popup hides.
- [ ] 5.12 Another tab or pane: with a menu open, switch tab (Ctrl+Tab) or focus another split pane: the popup hides,
      and does not appear over the other tab.
- [ ] 5.13 Scrolled back: with a menu open, scroll the terminal up with the mouse wheel until the prompt is out of view:
      the popup hides; scroll down, it comes back.
- [ ] 5.14 Select text with the mouse, then press Tab: the menu is drawn inline (a selection hides the cursor).
- [ ] 5.15 Bottom of the window: with the prompt on the last row, the popup opens above the line.
- [ ] 5.16 Scaling: on a monitor at 125 % or 150 %, the popup sits under the cursor (not offset towards the bottom
      right) and its text is sharp.
- [ ] 5.17 Two monitors with different scales: drag the window to the other monitor, press Tab: the popup is on that
      monitor, under the cursor.
- [ ] 5.18 tmux: `sudo apt install -y tmux; tmux`, then Tab: the menu is drawn inline (as on macOS and Linux).
- [ ] 5.19 ssh: `ssh localhost` (or any host) with goh on the far side: the menu is drawn inline.
- [ ] 5.20 `GOH_DISPLAY=inline zsh`, then Tab: inline. `exit` returns to the popup.
- [ ] 5.21 Without the Windows program: `GOH_POPUP=goh-popup-missing zsh`, then Tab: inline, and no error text.
- [ ] 5.22 Close the tab while a menu is open, then check Task Manager > Details: no `goh-popup.exe` left running.

**Report**: `goh-status` output, the step that failed, a screenshot. For a popup that does not appear, start
`GOH_DEBUG=1 zsh`, reproduce, and send the file named on `goh-status`'s `popup stderr:` line.

## 6. PowerShell native (not built yet)

Skipped until the daemon and `goh` build on Windows. To be checked then:

- [ ] 6.1 `goh doctor` in PowerShell 7 and in Windows PowerShell 5.1.
- [ ] 6.2 Tab completion through goh in both.
- [ ] 6.3 PowerShell 7.2+: as-you-type suggestions in PSReadLine's list view, marked `[goh]`.
- [ ] 6.4 The popup at the cursor for PowerShell's menu, with the same checks as 5.3 to 5.17.
- [ ] 6.5 The daemon starts on its own at the first Tab and survives closing the tab that started it.
- [ ] 6.6 Execution policy `Restricted` (Windows PowerShell 5.1's default): goh says how to allow the profile, and
      changes nothing itself.

## 7. Installer (not built yet)

Skipped until `install.ps1` and the Windows packages exist. To be checked then:

- [ ] 7.1 `irm <url>/install.ps1 | iex` installs goh for Windows and for the default WSL distro.
- [ ] 7.2 `curl ... | sh` inside WSL installs goh in the distro and `goh-popup.exe` on Windows.
- [ ] 7.3 The SHA-256 of each download is checked; a corrupted file stops the install.
- [ ] 7.4 Both PowerShell profiles (5.1 and 7) get exactly one goh block.
- [ ] 7.5 `--uninstall` from either side removes both sides, and nothing else.
- [ ] 7.6 Interop off in the distro: the WSL install succeeds and prints the PowerShell command for the Windows side.

## Done

- [ ] Sections 0 to 5 pass, or each failure is reported with its step number.
