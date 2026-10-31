# goh.ps1 — spec-driven completion for PowerShell (pwsh and Windows PowerShell) through
# Register-ArgumentCompleter -Native for Tab. On PowerShell 7.2+ with PSReadLine 2.2.2+, a predictor also shows goh's
# suggestions in PSReadLine's list as you type (compiled once into ~/.cache/goh/powershell, see Import-GohPredictor).
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

# Where gohd listens, as crates/goh-daemon/src/paths.rs decides it.
function Get-GohSocket {
    if ($env:GOH_SOCKET) { return $env:GOH_SOCKET }
    if ($env:XDG_RUNTIME_DIR) { return (Join-Path $env:XDG_RUNTIME_DIR 'goh/gohd.sock') }
    "/tmp/goh-$(& id -u)/gohd.sock"
}

# Assemblies Add-Type must reference for the predictor (found in the spike; empty when the defaults suffice).
$script:GohPredictorRefs = @()

# The predictor PSReadLine asks on every key. It must answer within 20 ms, so it asks gohd over its socket (about 1 ms)
# rather than starting `goh` (6-9 ms). The source is kept here so `goh init pwsh` carries it too.
$script:GohPredictorSource = @'
using System;
using System.Collections.Generic;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Management.Automation;
using System.Management.Automation.Subsystem;
using System.Management.Automation.Subsystem.Prediction;

namespace Goh
{
    public static class State
    {
        public static volatile string Socket = "";
        // PowerShell's location is not the process's working directory: goh.ps1 keeps this one up to date.
        public static volatile string Cwd = "/";
        // Its own session, apart from the Tab completer's ps-<pid>: gohd answers Stale to a generation older than the
        // session's latest, and `goh menu` counts generations from the clock.
        public static readonly string Session = "ps-" + Environment.ProcessId + "-predict";
    }

    public sealed class Predictor : ICommandPredictor
    {
        internal static readonly Guid Identifier = new Guid("6f3c2d8e-5a1b-4c7e-9d2f-0b8a7e6c5d41");
        private const int MaxFrame = 16 * 1024;
        private const int MaxSuggestions = 10;
        // Seeded from the clock (nanoseconds, as `goh menu` counts) so it is never behind what the daemon has seen.
        private static long _generation = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() * 1000000L;

        public Guid Id => Identifier;
        public string Name => "goh";
        public string Description => "Suggestions from goh's completion specs";

        public SuggestionPackage GetSuggestion(PredictionClient client, PredictionContext context, CancellationToken cancellationToken)
        {
            string line = context.InputAst.Extent.Text;
            if (string.IsNullOrWhiteSpace(line) || line.Contains('\n') || State.Socket.Length == 0) return default;
            try
            {
                var found = Ask(line, Interlocked.Increment(ref _generation));
                return found.Count == 0 ? default : new SuggestionPackage(found);
            }
            catch (Exception)
            {
                return default; // no daemon, too slow, or an answer that does not check out: show nothing
            }
        }

        private static List<PredictiveSuggestion> Ask(string line, long generation)
        {
            byte[] typed = Encoding.UTF8.GetBytes(line);
            byte[] request = JsonSerializer.SerializeToUtf8Bytes(new Dictionary<string, object>
            {
                ["type"] = "complete", ["v"] = 1, ["session_id"] = State.Session, ["generation"] = generation,
                ["shell"] = "pwsh", ["cwd"] = State.Cwd, ["buffer"] = line, ["cursor_byte"] = typed.Length,
                ["editor_state"] = "editing", ["live"] = true,
            });
            using var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
            socket.SendTimeout = 15;
            socket.ReceiveTimeout = 15;
            socket.Connect(new UnixDomainSocketEndPoint(State.Socket));
            byte[] length = BitConverter.GetBytes(request.Length);
            if (BitConverter.IsLittleEndian) Array.Reverse(length);
            socket.Send(length);
            socket.Send(request);
            byte[] head = Read(socket, 4);
            if (BitConverter.IsLittleEndian) Array.Reverse(head);
            int size = BitConverter.ToInt32(head, 0);
            if (size <= 0 || size > MaxFrame) return new List<PredictiveSuggestion>();
            using var reply = JsonDocument.Parse(Read(socket, size));
            var found = new List<PredictiveSuggestion>();
            var seen = new HashSet<string>();
            if (!reply.RootElement.TryGetProperty("type", out var type) || type.GetString() != "results") return found;
            foreach (var c in reply.RootElement.GetProperty("suggestions").EnumerateArray())
            {
                int start = c.GetProperty("replace_start_byte").GetInt32();
                int end = c.GetProperty("replace_end_byte").GetInt32();
                string insert = c.GetProperty("insert_text").GetString() ?? "";
                if (start < 0 || start > end || end > typed.Length || insert.Length == 0 || HasControl(insert)) continue;
                string whole = Encoding.UTF8.GetString(typed, 0, start) + insert + Encoding.UTF8.GetString(typed, end, typed.Length - end);
                if (whole == line || !seen.Add(whole)) continue;
                string tip = c.TryGetProperty("description", out var d) ? SingleLine(d.GetString()) : null;
                found.Add(new PredictiveSuggestion(whole, tip));
                if (found.Count == MaxSuggestions) break;
            }
            return found;
        }

        // Never trust the wire: like the goh client, drop an insertion with a control character in it (an escape sequence
        // or a newline would act on the terminal or run the line) and blank them out of the tooltip.
        private static bool HasControl(string s)
        {
            foreach (char ch in s) if (char.IsControl(ch)) return true;
            return false;
        }

        private static string SingleLine(string s)
        {
            if (s == null || !HasControl(s)) return s;
            var chars = s.ToCharArray();
            for (int i = 0; i < chars.Length; i++) if (char.IsControl(chars[i])) chars[i] = ' ';
            return new string(chars);
        }

        private static byte[] Read(Socket socket, int count)
        {
            var buffer = new byte[count];
            for (int got = 0; got < count;)
            {
                int n = socket.Receive(buffer, got, count - got, SocketFlags.None);
                if (n == 0) throw new SocketException();
                got += n;
            }
            return buffer;
        }

        public bool CanAcceptFeedback(PredictionClient client, PredictorFeedbackKind feedback) => false;
        public void OnSuggestionDisplayed(PredictionClient client, uint session, int countOrIndex) { }
        public void OnSuggestionAccepted(PredictionClient client, uint session, string acceptedSuggestion) { }
        public void OnCommandLineAccepted(PredictionClient client, IReadOnlyList<string> history) { }
        public void OnCommandLineExecuted(PredictionClient client, string commandLine, bool success) { }
    }

    public sealed class Init : IModuleAssemblyInitializer, IModuleAssemblyCleanup
    {
        public void OnImport() => SubsystemManager.RegisterSubsystem(SubsystemKind.CommandPredictor, new Predictor());
        public void OnRemove(PSModuleInfo module) => SubsystemManager.UnregisterSubsystem(SubsystemKind.CommandPredictor, Predictor.Identifier);
    }
}
'@

# Compile the predictor once per version of its source, then load it. $false (and nothing changed) on PowerShell
# older than 7.2 or on any error. The compiled file is written under a temporary name and moved into place, so a
# half-written dll is never loaded; a cached dll that will not load is deleted and compiled once more.
function Import-GohPredictor {
    if ($PSVersionTable.PSVersion -lt [version]'7.2') { return $false }
    if ('Goh.Predictor' -as [type]) { return $true }
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($script:GohPredictorSource)
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).Substring(0, 16)
        $cacheRoot = if ($env:XDG_CACHE_HOME) { $env:XDG_CACHE_HOME } else { Join-Path $HOME '.cache' }
        $cache = Join-Path $cacheRoot 'goh/powershell'
        $dll = Join-Path $cache "GohPredictor-$hash.dll"
        $loaded = $false
        foreach ($attempt in 1..2) {
            if (-not (Test-Path -LiteralPath $dll)) {
                New-Item -ItemType Directory -Force -Path $cache | Out-Null
                $temp = "$dll.$PID.tmp"
                try {
                    $refs = @{}
                    if ($script:GohPredictorRefs.Count) { $refs.ReferencedAssemblies = $script:GohPredictorRefs }
                    Add-Type -TypeDefinition $script:GohPredictorSource -OutputAssembly $temp -OutputType Library @refs
                    Move-Item -LiteralPath $temp -Destination $dll -Force
                } finally {
                    Remove-Item -LiteralPath $temp -Force -ErrorAction SilentlyContinue
                }
            }
            try {
                Import-Module $dll -ErrorAction Stop
                $loaded = $true
                break
            } catch {
                Remove-Item -LiteralPath $dll -Force -ErrorAction SilentlyContinue
            }
        }
        if (-not $loaded) { return $false }
        [Goh.State]::Socket = Get-GohSocket
        [Goh.State]::Cwd = (Get-Location).ProviderPath
        return $true
    } catch {
        return $false
    }
}

# GOH_LIVE as the shell sees it: the environment variable if you set one, else display.live from the config file
# (`goh config env` prints NAME=value lines, as zsh and bash read them). Empty when neither says. Never throws.
function Get-GohLive {
    try {
        if ($env:GOH_LIVE) { return $env:GOH_LIVE }
        foreach ($line in @(& goh config env 2>$null)) {
            if ([string]$line -match '^GOH_LIVE=(.*)$') { return $Matches[1].Trim() }
        }
    } catch { }
    return ''
}

# Show the predictor's list as you type: add goh to PSReadLine's prediction sources and use the list view. Nothing
# at all when GOH_LIVE=off or display.live is false. The user's own choice of source is kept, only widened. Never
# throws.
function Enable-GohPredictor {
    try {
        if ((Get-GohLive) -eq 'off') { return }
        $psrl = Get-Module PSReadLine
        if (-not $psrl -or $psrl.Version -lt [version]'2.2.2') { return }
        if (-not (Import-GohPredictor)) { return }
        # Start the daemon only when its socket is missing: no process per shell start while it is up.
        if (-not (Test-Path -LiteralPath (Get-GohSocket))) { & goh start *> $null }
        $source = (Get-PSReadLineOption).PredictionSource.ToString()
        $wider = switch ($source) { 'None' { 'Plugin' } 'History' { 'HistoryAndPlugin' } default { $source } }
        Set-PSReadLineOption -PredictionSource $wider
        Set-PSReadLineOption -PredictionViewStyle ListView
    } catch { }
}

# Directory history for `cd` suggestions when zoxide is not installed; the predictor's working directory.
$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = {
    & goh visit (Get-Location).ProviderPath *> $null
    if ('Goh.State' -as [type]) { [Goh.State]::Cwd = (Get-Location).ProviderPath }
}

Register-GohCompleters
Enable-GohPredictor
