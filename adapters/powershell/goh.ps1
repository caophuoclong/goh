# goh.ps1 — spec-driven completion for PowerShell (pwsh and Windows PowerShell) through
# Register-ArgumentCompleter -Native. No key bindings are touched; PSReadLine keeps Tab and the menu.
# Dot-source it from $PROFILE:  . /path/to/goh.ps1
# Fail-open: any error returns no candidates and PowerShell falls back to its own completion.

$script:GohSession = "ps-$PID"

$script:GohComplete = {
    param($wordToComplete, $commandAst, $cursorPosition)
    try {
        $text = $commandAst.Extent.Text
        $offset = [Math]::Min([Math]::Max($cursorPosition - $commandAst.Extent.StartOffset, 0), $text.Length)
        $before = $text.Substring(0, $offset)
        if ($wordToComplete -eq '' -and -not $before.EndsWith(' ')) { $before += ' ' }
        $utf8 = [System.Text.Encoding]::UTF8
        $point = $utf8.GetByteCount($before)
        $curBytes = $utf8.GetByteCount($wordToComplete)
        $out = & goh menu --session $script:GohSession --cwd (Get-Location).ProviderPath --limit 50 --cursor-byte $point --buffer $before 2>$null
        if ($LASTEXITCODE -ne 0) { return }
        foreach ($line in $out) {
            $f = $line.Split([char]0x1f)
            if ([int]$f[0] -ne ($point - $curBytes) -or [int]$f[1] -ne $point) { continue }
            $insert = $f[5].TrimEnd(' ').Replace('\ ', ' ')
            if ($insert -match '\s') { $insert = "'" + $insert.Replace("'", "''") + "'" }
            $desc = if ($f[6]) { $f[6] } else { $f[4] }
            [System.Management.Automation.CompletionResult]::new($insert, $f[4], 'ParameterValue', $desc)
        }
    } catch { }
}

function Register-GohCompleters {
    $names = & goh specs 2>$null | ForEach-Object { ($_ -split "`t")[0] } | Where-Object { $_ }
    if ($names) { Register-ArgumentCompleter -Native -CommandName $names -ScriptBlock $script:GohComplete }
}

# Directory history for `cd` suggestions when zoxide is not installed.
$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = {
    & goh visit (Get-Location).ProviderPath *> $null
}

Register-GohCompleters
