# Live check of the Windows Terminal caret anchor and the popup window. Run it at a PowerShell prompt in a Windows
# Terminal pane (not over ssh), from the repository after `cargo build --release -p goh-popup`, or from the unpacked
# Windows test kit (which has this script, its sample frame and goh-popup.exe):
#
#   .\overlay-demo.ps1 [-Popup path\to\goh-popup.exe]
#
# 1. prints what the gate decides for this terminal (`goh-popup.exe --resolve`);
# 2. writes "PS> git c" without a newline, so the cursor sits right after it, and shows the popup for about 6 s:
#    it should appear just below that line (above it on the bottom rows), and typing focus must stay in the terminal.
# Report: the printed decisions, and whether the popup was where the cursor is.
param([string]$Popup)
$ErrorActionPreference = 'Stop'
if (-not $Popup) {
    # Next to this script (the kit), then the repository's release build, then PATH.
    $Popup = @(
        (Join-Path $PSScriptRoot 'goh-popup.exe'),
        (Join-Path $PSScriptRoot '..\..\..\target\release\goh-popup.exe')
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $Popup) { $Popup = (Get-Command goh-popup.exe).Source }
}
$exe = (Resolve-Path $Popup).Path
$frame = (Get-Content -Raw (Join-Path $PSScriptRoot 'sample-frame-auto.json')) -replace "`r?`n", ' '

Write-Host "--- popup: $exe"
Write-Host '--- gate for this terminal:'
& $exe --resolve

Write-Host '--- showing the popup at the cursor for about 6 seconds... (do not press keys)'
[Console]::Write('PS> git c')
$start = New-Object Diagnostics.ProcessStartInfo $exe
$start.UseShellExecute = $false
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$popupProcess = [Diagnostics.Process]::Start($start)
$popupProcess.StandardInput.WriteLine($frame)
$popupProcess.StandardInput.Flush()
Start-Sleep -Seconds 6
$popupProcess.StandardInput.WriteLine('{"type":"quit"}')
$popupProcess.StandardInput.Close()
$decision = $popupProcess.StandardOutput.ReadToEnd()
[void]$popupProcess.WaitForExit(5000)
Write-Host ''
Write-Host '--- popup decision:'
$decision
