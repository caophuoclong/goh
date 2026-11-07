# Install a Windows test kit built by CI (the `goh-windows-test-kit` artifact) without building anything:
# goh-popup.exe on Windows, and goh for zsh in a WSL distro. Run it from the unpacked kit folder:
#
#   powershell -ExecutionPolicy Bypass -File .\install-test-kit.ps1 [-Distro Ubuntu]
#
# Windows: goh-popup.exe goes to %LOCALAPPDATA%\Programs\goh, which is added to your user PATH (WSL appends that PATH
# to its own when it starts). WSL: install.sh from the kit installs the kit's Linux package, checked against its
# .sha256, and adds the goh block to ~/.zshrc. Nothing else is changed.
param([string]$Distro)
$ErrorActionPreference = 'Stop'
$kit = $PSScriptRoot

$dir = Join-Path $env:LOCALAPPDATA 'Programs\goh'
New-Item -ItemType Directory -Force $dir | Out-Null
$exe = Join-Path $dir 'goh-popup.exe'
Copy-Item (Join-Path $kit 'goh-popup.exe') $exe -Force
Unblock-File $exe # a kit downloaded with a browser is marked as from the internet
Write-Host "goh: goh-popup.exe installed in $dir"
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
    [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ";$dir").TrimStart(';')), 'User')
    Write-Host "goh: added $dir to your PATH"
}

$package = Get-ChildItem $kit -Filter 'goh-*-x86_64-unknown-linux-gnu.tar.gz' | Select-Object -First 1
if (-not $package) { throw 'the kit has no WSL package (goh-*-x86_64-unknown-linux-gnu.tar.gz)' }
$on = @()
if ($Distro) { $on = @('-d', $Distro) }
& wsl.exe @on -- sh -c 'command -v zsh >/dev/null'
if ($LASTEXITCODE -ne 0) { throw 'zsh is not installed in WSL: run `sudo apt install -y zsh` there, then this again' }
$kitInWsl = (& wsl.exe @on -- wslpath -a $kit | Out-String).Trim()
if (-not $kitInWsl) { throw "WSL could not see $kit" }
& wsl.exe @on -- env GOH_SHELL=zsh sh "$kitInWsl/install.sh" --package "$kitInWsl/$($package.Name)"
if ($LASTEXITCODE -ne 0) { throw 'installing goh in WSL failed (output above)' }
# No quotes inside the argument: Windows PowerShell 5.1 drops them when it starts a program.
$login = (& wsl.exe @on -- sh -c 'basename $SHELL' | Out-String).Trim()
if ($login -ne 'zsh') {
    Write-Host "goh: your WSL login shell is $login; make it zsh with: chsh -s `"`$(command -v zsh)`"  (in WSL)"
}
Write-Host 'goh: done. Close every Windows Terminal window, run `wsl --shutdown`, then open an Ubuntu tab and type: git ch'
