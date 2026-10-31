# PowerShell and Nushell Menus as You Type Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PowerShell and Nushell users get goh's suggestions in a list that opens while they type, drawn by the shell itself: PSReadLine's prediction list in PowerShell, a bordered IDE-style menu in Nushell.

**Architecture:** Neither shell lets goh draw its own box, but both have one built in. Each part only feeds goh's candidates to it:

- **PowerShell (Part A).** PSReadLine 2.2.2+ on PowerShell 7.2+ shows, with `PredictionViewStyle ListView`, a list under the line that updates on every key, filled by *predictor* plugins. A predictor must be a compiled .NET class implementing `ICommandPredictor` and answer within **20 ms**. `goh.ps1` carries the C# source as a string, compiles it once with `Add-Type` into `~/.cache/goh/powershell/`, and imports it. The predictor talks to `gohd` over its Unix socket directly: a `goh menu` process per key takes 6–9 ms on its own, too close to 20 ms; the socket round trip is about 1 ms. The C# client ships in the same package as `gohd`, so the protocol stays a same-version internal one (CONTRIBUTING.md).
- **Nushell (Part B).** reedline has an `ide` menu layout (bordered, next to the cursor, descriptions on the right), menus can take a `source` closure, and a key binding can run several events in sequence. `goh.nu` adds a menu `goh_menu` whose source asks `goh menu`, and binds Space to "insert a space, then open `goh_menu`", so the menu opens at each new word and filters as you keep typing. Nothing opens a menu by itself in reedline (the auto-menu PR, reedline #993, was closed); `completions.persistent_menus` (reedline #1118, nushell #18573) keeps it open while you erase, where available.

Both parts are independent: either can ship alone.

**Tech Stack:** C# (compiled at runtime by PowerShell's `Add-Type`, no .NET SDK needed), PowerShell 7.2+, Nushell, Rust tests that drive the real shells.

**Spec:** new `docs/features/powershell-adapter.md` and `docs/features/nushell-adapter.md`; `docs/features/other-shells.md`; `docs/features/README.md` (index).

## Global Constraints

- Every user-visible change updates its spec in `docs/features/` in the same commit series: text, `updated: <today>`, and a `- unreleased: ...` line under `## Changes`. A new feature gets a new spec and a row in `docs/features/README.md`. Run `scripts/check-feature-specs.sh` before the last commit.
- Conventional Commits (`feat(pwsh): ...`, `feat(nu): ...`); end every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Fail-open everywhere: an old shell, a missing PSReadLine, a compile error, no daemon or a slow answer leaves today's Tab completion working and shows nothing new.
- `goh.ps1` must still parse and complete on Tab in Windows PowerShell 5.1: no `??`, `?.` or other 7-only syntax outside strings.
- The user's own shell settings are only widened, never replaced: a PSReadLine prediction source gets `Plugin` added; Nushell menus and key bindings are appended.
- Before Part A or Part B code, its spike task must pass on a real shell; if a spike fails, stop that part and report what failed.

---

## File Structure

| File | Change |
|---|---|
| `adapters/powershell/goh.ps1` | C# predictor source, compile-once cache, PSReadLine options |
| `crates/goh-daemon/tests/powershell_predictor.rs` | The predictor against a test daemon, without a terminal |
| `adapters/nushell/goh.nu` | `goh-menu-source`, `goh-ui` |
| `crates/goh-daemon/tests/nushell_menu.rs` | Nushell on a pseudo-terminal: Space opens the box |
| `.github/workflows/ci.yml` | Install nushell (PowerShell is already on the macOS runner) |
| `docs/features/powershell-adapter.md`, `docs/features/nushell-adapter.md` | New specs |
| `docs/features/other-shells.md`, `docs/features/README.md` | Narrowed spec, index rows |

---

## Part A: PowerShell

### Task A1: Spike on a real PowerShell

**Files:** none (scratch only).

- [ ] **Step 1: Check the versions**

```bash
brew install --cask powershell   # skip if `pwsh` exists
pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString(); (Get-Module -ListAvailable PSReadLine | Sort-Object Version -Descending | Select-Object -First 1).Version.ToString()'
```

Expected: PowerShell 7.2 or newer, PSReadLine 2.2.2 or newer. If PSReadLine is older: `pwsh -Command 'Install-Module PSReadLine -MinimumVersion 2.2.2 -Scope CurrentUser -Force'`.

- [ ] **Step 2: Compile a predictor with `Add-Type` and load it as a module**

Save as `$TMPDIR/spike.ps1` (scratch, not in the repo):

```powershell
$src = @'
using System; using System.Collections.Generic; using System.Threading;
using System.Management.Automation; using System.Management.Automation.Subsystem; using System.Management.Automation.Subsystem.Prediction;
namespace Spike {
  public sealed class P : ICommandPredictor {
    public static readonly Guid G = new Guid("0f0e0d0c-0b0a-4908-8706-050403020100");
    public Guid Id => G; public string Name => "spike"; public string Description => "spike";
    public SuggestionPackage GetSuggestion(PredictionClient c, PredictionContext x, CancellationToken t)
      => new SuggestionPackage(new List<PredictiveSuggestion> { new PredictiveSuggestion(x.InputAst.Extent.Text + " SPIKE", "tip") });
    public bool CanAcceptFeedback(PredictionClient c, PredictorFeedbackKind k) => false;
    public void OnSuggestionDisplayed(PredictionClient c, uint s, int n) { }
    public void OnSuggestionAccepted(PredictionClient c, uint s, string a) { }
    public void OnCommandLineAccepted(PredictionClient c, IReadOnlyList<string> h) { }
    public void OnCommandLineExecuted(PredictionClient c, string l, bool ok) { }
  }
  public sealed class Init : IModuleAssemblyInitializer, IModuleAssemblyCleanup {
    public void OnImport() => SubsystemManager.RegisterSubsystem(SubsystemKind.CommandPredictor, new P());
    public void OnRemove(PSModuleInfo m) => SubsystemManager.UnregisterSubsystem(SubsystemKind.CommandPredictor, P.G);
  }
}
'@
$dll = Join-Path ([IO.Path]::GetTempPath()) 'spike-predictor.dll'
Add-Type -TypeDefinition $src -OutputAssembly $dll -OutputType Library
Import-Module $dll
$ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git chec')
$client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('spike', 'Terminal')
([Spike.P]::new()).GetSuggestion($client, $ctx, [Threading.CancellationToken]::None).SuggestionEntries.SuggestionText
```

Run: `pwsh -NoProfile -File $TMPDIR/spike.ps1`
Expected: `git chec SPIKE`. Then in an interactive `pwsh`, run `. $TMPDIR/spike.ps1; Set-PSReadLineOption -PredictionSource HistoryAndPlugin -PredictionViewStyle ListView`, type `git chec`, and check that a list under the line shows `git chec SPIKE [spike]`.

- [ ] **Step 3: Check that `Add-Type` can reference what the predictor needs**

Add `using System.Net.Sockets; using System.Text.Json;` to the spike source and `var s = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified); JsonDocument.Parse("{}");` inside `GetSuggestion`, delete the old dll, run again. If it fails with "type or namespace not found", add `-ReferencedAssemblies System.Net.Sockets, System.Net.Primitives, System.Text.Json, System.Collections, System.Runtime, System.Memory` to `Add-Type` and run again. Record the exact list that works: Task A2 uses it as `$script:GohPredictorRefs` (an empty array if none was needed).

---

### Task A2: The predictor

**Files:**
- Modify: `adapters/powershell/goh.ps1`
- Test: `crates/goh-daemon/tests/powershell_predictor.rs`

**Interfaces:**
- Produces: .NET types `Goh.State` (static `Socket`, `Cwd`, `Session`) and `Goh.Predictor : ICommandPredictor` (public parameterless constructor, `Name` = `goh`). `GetSuggestion` returns up to 10 whole-line suggestions: the typed line with a candidate's replacement applied, tooltip = the candidate's description. PowerShell function `Import-GohPredictor` (returns `$true` once `Goh.Predictor` is loaded).

- [ ] **Step 1: Write the failing test**

`crates/goh-daemon/tests/powershell_predictor.rs`:

```rust
//! adapters/powershell/goh.ps1's predictor in a real pwsh, against a test daemon: the suggestions PSReadLine would
//! show in its list. No terminal needed. Skipped (with a note) when pwsh 7.2+ or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};

use common::{utf8_locale, Fixture};

fn pwsh() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("pwsh")).find(|f| f.is_file()))
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/powershell/goh.ps1")
}

#[test]
fn the_predictor_suggests_whole_lines_from_goh() {
    let (Some(pwsh), Some(locale)) = (pwsh(), utf8_locale()) else {
        eprintln!("skipping: needs pwsh and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let script = format!(
        ". '{}'
         if (-not (Import-GohPredictor)) {{ 'NO PREDICTOR'; exit 1 }}
         $ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git chec')
         $client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('test', 'Terminal')
         $pkg = ([Goh.Predictor]::new()).GetSuggestion($client, $ctx, [Threading.CancellationToken]::None)
         foreach ($s in $pkg.SuggestionEntries) {{ \"$($s.SuggestionText)`t$($s.ToolTip)\" }}",
        adapter().display()
    );
    let out = fx.command(pwsh.to_str().unwrap(), &["-NoProfile", "-NonInteractive", "-Command", &script], &[]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    let first = text.lines().next().unwrap_or_default();
    assert!(first.starts_with("git checkout"), "the whole line, with the word completed: {text}");
    assert!(first.contains('\t') && !first.ends_with('\t'), "with the description as its tooltip: {text}");
}

#[test]
fn no_daemon_means_no_suggestions_and_no_error() {
    let (Some(pwsh), Some(locale)) = (pwsh(), utf8_locale()) else {
        return;
    };
    let fx = Fixture::new(locale, false);
    let script = format!(
        ". '{}'
         if (-not (Import-GohPredictor)) {{ exit 1 }}
         [Goh.State]::Socket = '/nonexistent/gohd.sock'
         $ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git chec')
         $client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('test', 'Terminal')
         $pkg = ([Goh.Predictor]::new()).GetSuggestion($client, $ctx, [Threading.CancellationToken]::None)
         'COUNT ' + @($pkg.SuggestionEntries).Count",
        adapter().display()
    );
    let out = fx.command(pwsh.to_str().unwrap(), &["-NoProfile", "-NonInteractive", "-Command", &script], &[]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("COUNT 0"), "{}", String::from_utf8_lossy(&out.stderr));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --test powershell_predictor`
Expected: FAIL with `Import-GohPredictor` not recognized.

- [ ] **Step 3: Add the predictor to `goh.ps1`**

Update the header comment's second line to:

```powershell
# Register-ArgumentCompleter -Native for Tab. On PowerShell 7.2+ with PSReadLine 2.2.2+, a predictor also shows goh's
# suggestions in PSReadLine's list as you type (compiled once into ~/.cache/goh/powershell, see Import-GohPredictor).
```

Insert before `Register-GohCompleters` at the end of the file:

```powershell
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
        public static readonly string Session = "ps-" + Environment.ProcessId;
    }

    public sealed class Predictor : ICommandPredictor
    {
        internal static readonly Guid Identifier = new Guid("6f3c2d8e-5a1b-4c7e-9d2f-0b8a7e6c5d41");
        private const int MaxFrame = 16 * 1024;
        private const int MaxSuggestions = 10;
        private static long _generation;

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
                if (start < 0 || start > end || end > typed.Length || insert.IndexOfAny(new[] { '\n', '\r' }) >= 0) continue;
                string whole = Encoding.UTF8.GetString(typed, 0, start) + insert + Encoding.UTF8.GetString(typed, end, typed.Length - end);
                if (whole == line || !seen.Add(whole)) continue;
                string tip = c.TryGetProperty("description", out var d) ? d.GetString() : null;
                found.Add(new PredictiveSuggestion(whole, tip));
                if (found.Count == MaxSuggestions) break;
            }
            return found;
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
# older than 7.2 or on any error.
function Import-GohPredictor {
    if ($PSVersionTable.PSVersion -lt [version]'7.2') { return $false }
    if ('Goh.Predictor' -as [type]) { return $true }
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($script:GohPredictorSource)
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).Substring(0, 16)
        $cacheRoot = if ($env:XDG_CACHE_HOME) { $env:XDG_CACHE_HOME } else { Join-Path $HOME '.cache' }
        $cache = Join-Path $cacheRoot 'goh/powershell'
        $dll = Join-Path $cache "GohPredictor-$hash.dll"
        if (-not (Test-Path $dll)) {
            New-Item -ItemType Directory -Force -Path $cache | Out-Null
            $refs = @{}
            if ($script:GohPredictorRefs.Count) { $refs.ReferencedAssemblies = $script:GohPredictorRefs }
            Add-Type -TypeDefinition $script:GohPredictorSource -OutputAssembly $dll -OutputType Library @refs
        }
        Import-Module $dll -ErrorAction Stop
        [Goh.State]::Socket = Get-GohSocket
        [Goh.State]::Cwd = (Get-Location).ProviderPath
        return $true
    } catch {
        return $false
    }
}

# Show the predictor's list as you type: add goh to PSReadLine's prediction sources and use the list view, unless
# GOH_LIVE=off (display.live). The user's own choice of source is kept, only widened.
function Enable-GohPredictor {
    $psrl = Get-Module PSReadLine
    if (-not $psrl -or $psrl.Version -lt [version]'2.2.2') { return }
    if (-not (Import-GohPredictor)) { return }
    & goh start *> $null
    $source = (Get-PSReadLineOption).PredictionSource.ToString()
    $wider = switch ($source) { 'None' { 'Plugin' } 'History' { 'HistoryAndPlugin' } default { $source } }
    Set-PSReadLineOption -PredictionSource $wider
    if ($env:GOH_LIVE -ne 'off') { Set-PSReadLineOption -PredictionViewStyle ListView }
}
```

Replace the `LocationChangedAction` block with:

```powershell
# Directory history for `cd` suggestions when zoxide is not installed; the predictor's working directory.
$ExecutionContext.SessionState.InvokeCommand.LocationChangedAction = {
    & goh visit (Get-Location).ProviderPath *> $null
    if ('Goh.State' -as [type]) { [Goh.State]::Cwd = (Get-Location).ProviderPath }
}
```

and the last line with:

```powershell
Register-GohCompleters
Enable-GohPredictor
```

Set `$script:GohPredictorRefs` to the list the spike recorded (Task A1 Step 3).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p goh-daemon --test powershell_predictor`
Expected: PASS (the first run compiles the predictor, a few seconds).

- [ ] **Step 5: Check the 20 ms budget by hand**

In an interactive `pwsh` with goh running: `. ./adapters/powershell/goh.ps1`, type `git chec`, `docker run --`, `kubectl get `. Expected: a list under the line with `[goh]` entries, updating on every key. Then run:

```powershell
$ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git checkout ')
$client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('t', 'Terminal')
$p = [Goh.Predictor]::new()
(Measure-Command { 1..50 | ForEach-Object { $p.GetSuggestion($client, $ctx, [Threading.CancellationToken]::None) } }).TotalMilliseconds / 50
```

Expected: under 10 ms per call. Note the number in the commit message.

- [ ] **Step 6: Windows PowerShell 5.1 still parses the file**

Run: `pwsh -NoProfile -Command '$null = [System.Management.Automation.Language.Parser]::ParseFile("adapters/powershell/goh.ps1", [ref]$null, [ref]$e); $e.Count'`
Expected: `0`. Then check by eye that no `??`, `?.` or `&&` appears outside the C# here-string.

- [ ] **Step 7: Commit**

```bash
git add adapters/powershell/goh.ps1 crates/goh-daemon/tests/powershell_predictor.rs
git commit -m "feat(pwsh): suggestions as you type in PSReadLine's list"
```

---

### Task A3: PowerShell spec

**Files:**
- Create: `docs/features/powershell-adapter.md`
- Modify: `docs/features/other-shells.md`, `docs/features/README.md`

- [ ] **Step 1: Write the spec**

`docs/features/powershell-adapter.md`:

```markdown
---
id: powershell-adapter
title: PowerShell: suggestions as you type
status: experimental
since: unreleased
updated: <today>
area: shells
shells: [pwsh]
settings: [display.live]
commands: [goh init pwsh]
env: [GOH_LIVE]
files: [~/.cache/goh/powershell/]
---

# PowerShell: suggestions as you type

On PowerShell 7.2+ with PSReadLine 2.2.2+, goh's suggestions appear in PSReadLine's list under the line while you
type, marked `[goh]`, next to your history. Load it with `. /path/to/goh.ps1` in `$PROFILE`.

## Behaviour

- Each suggestion is the whole line with the current word completed; ↑↓ pick one, Enter runs it. Its description
  shows as the tooltip.
- goh adds itself to PSReadLine's prediction sources (`Plugin`, or `HistoryAndPlugin` when history was on) and
  switches to the list view; `display.live: false` (GOH_LIVE=off) keeps your view style. F2 switches views.
- The first load compiles a small predictor into `~/.cache/goh/powershell/` (a few seconds, once per goh version).
- Tab still completes through goh as before, in every PowerShell including Windows PowerShell 5.1.

## Limits

- No popup and no floating box: the list is PSReadLine's own.
- A suggestion that takes longer than PSReadLine's 20 ms is not shown.
- Older PowerShell or PSReadLine: Tab completion only.

## Changes

- unreleased: first release.
```

(`<today>` is the date of the commit.)

In `docs/features/other-shells.md`: remove `PowerShell` from the title and the first sentence, remove `pwsh` from `shells:`, set `updated:`, and add under `## Changes`: `- unreleased: PowerShell has its own spec (powershell-adapter).`

In `docs/features/README.md`, add after the `other-shells` row:

```markdown
| [powershell-adapter](powershell-adapter.md) | shells | experimental | unreleased |
```

- [ ] **Step 2: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add docs/features
git commit -m "docs: describe PowerShell suggestions as you type"
```

---

## Part B: Nushell

### Task B1: Spike on a real Nushell

**Files:** none (scratch only).

- [ ] **Step 1: Install and check the version**

```bash
brew install nushell
nu --version
nu -n -c '$env.config.completions | columns | to text'
```

Record whether `persistent_menus` is in that list.

- [ ] **Step 2: Try a source menu with the ide layout and a Space binding**

Start `nu -n` (no config) and paste:

```nu
$env.config.menus = ($env.config.menus | append {
    name: spike_menu
    only_buffer_difference: false
    marker: ""
    type: { layout: ide, border: true, description_mode: "prefer_right", correct_cursor_pos: true }
    style: { text: default, selected_text: { attr: r }, description_text: dark_gray }
    source: {|buffer, position| [{ value: "checkout", description: "Switch branches", span: { start: $position, end: $position } }] }
})
$env.config.keybindings = ($env.config.keybindings | append {
    name: spike_space, modifier: none, keycode: space, mode: [emacs vi_insert]
    event: [{ edit: insertstring value: " " }, { send: menu name: spike_menu }]
})
```

Type `git ` then `c`, Down, Enter. Expected: Space opens a bordered box under the cursor with `checkout  Switch branches`; typing `c` keeps it open; Enter inserts `checkout`. Record, for Task B2:

- whether `only_buffer_difference` is accepted or nushell asks for `input_mode: cursor_prefix` instead;
- whether typing a letter keeps the box open and calls the source again (add `print -e $buffer` to the closure to see);
- which key moves in the box (Down or Tab) and whether Esc closes it.

If no box appears, stop Part B and report what nushell printed.

---

### Task B2: The box in `goh.nu`

**Files:**
- Modify: `adapters/nushell/goh.nu`
- Test: `crates/goh-daemon/tests/nushell_menu.rs`

**Interfaces:**
- Produces: `goh-menu-source [buffer: string, position: int]` returning `list<record<value, description, span>>`, and `goh-ui` (`def --env`), which appends menu `goh_menu` and key binding `goh_space` and turns on `completions.persistent_menus` where it exists.

- [ ] **Step 1: Write the failing test**

`crates/goh-daemon/tests/nushell_menu.rs`:

```rust
//! adapters/nushell/goh.nu in a real nushell on a pseudo-terminal: Space opens goh's box with the next word's
//! candidates. Skipped (with a note) when nu or a UTF-8 locale is missing.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{utf8_locale, Fixture, Pty};

fn nu() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("nu")).find(|f| f.is_file()))
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/nushell/goh.nu")
}

/// What the terminal shows: `out` without CSI and OSC sequences.
fn screen(out: &[u8]) -> String {
    let s = String::from_utf8_lossy(out);
    let (mut text, mut chars) = (String::new(), s.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\x1b', Some('[')) => {
                chars.next();
                while chars.next().is_some_and(|c| !('@'..='~').contains(&c)) {}
            }
            ('\x1b', Some(']')) => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => text.push(c),
        }
    }
    text
}

#[test]
fn space_opens_a_box_of_goh_candidates() {
    let (Some(nu), Some(locale)) = (nu(), utf8_locale()) else {
        eprintln!("skipping: needs nu and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    let mut sh = Pty::spawn(fx.command(nu.to_str().unwrap(), &["-n"], &[]));
    sh.drain(Duration::from_millis(800), Duration::from_secs(5));
    sh.send(&format!("source '{}'\r", adapter().display()), 800);
    sh.send("goh-ui\r", 800);
    sh.drain(Duration::from_millis(300), Duration::from_secs(2));
    sh.send("git", 300);
    sh.send(" ", 0);
    let shown = screen(&sh.drain(Duration::from_millis(800), Duration::from_secs(4)));
    assert!(shown.contains("checkout"), "the box lists git's subcommands: {shown}");
    assert!(shown.contains("Switch branches"), "with their descriptions: {shown}");
    assert!(["╭", "┌", "│"].iter().any(|b| shown.contains(b)), "inside a border: {shown}");
}
```

If `Pty::send` or `Pty::drain` differ from these signatures, check `crates/goh-daemon/tests/common/mod.rs` and adapt the calls; the assertions stay.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p goh-daemon --test nushell_menu`
Expected: FAIL: `goh-ui` is not a command, so no box.

- [ ] **Step 3: Add the menu to `goh.nu`**

Extend the header comment with:

```nu
# A box as you type: after `source`, run `goh-ui` in config.nu. Space then opens a bordered menu of goh's candidates
# for the next word, which filters as you keep typing; Down/Up move, Enter takes one, Esc closes it.
```

Append to the file (use the `only_buffer_difference` / `input_mode` key the spike accepted):

```nu
# Candidates for the box: the line and the cursor (a byte offset) as nushell gives them to a menu's source.
export def goh-menu-source [buffer: string, position: int] {
    let sep = (char unit_separator)
    let rows = (do { ^goh menu --live --session $"nu-($nu.pid)" --cwd $env.PWD --limit 50 --cursor-byte ($position | into string) --buffer $buffer } | complete)
    if $rows.exit_code != 0 { return [] }
    $rows.stdout | lines | each {|line|
        let f = ($line | split row $sep)
        if ($f | length) >= 7 {
            {
                value: ($f.5 | str trim --right | str replace --all '\ ' ' ')
                description: ($f.6? | default '')
                span: { start: ($f.0 | into int), end: ($f.1 | into int) }
            }
        }
    } | compact
}

# Add goh's box: menu `goh_menu` and a Space binding that opens it. Your own menus and bindings stay.
export def --env goh-ui [] {
    if ('persistent_menus' in ($env.config.completions | columns)) {
        $env.config.completions.persistent_menus = true
    }
    $env.config.menus = ($env.config.menus | append {
        name: goh_menu
        only_buffer_difference: false
        marker: ""
        type: {
            layout: ide
            min_completion_width: 0
            max_completion_width: 40
            max_completion_height: 10
            padding: 0
            border: true
            cursor_offset: 0
            description_mode: "prefer_right"
            min_description_width: 0
            max_description_width: 50
            max_description_height: 10
            description_offset: 1
            correct_cursor_pos: true
        }
        style: {
            text: default
            selected_text: { attr: r }
            description_text: dark_gray
            match_text: { attr: u }
            selected_match_text: { attr: ur }
        }
        source: {|buffer, position| goh-menu-source $buffer $position }
    })
    $env.config.keybindings = ($env.config.keybindings | append {
        name: goh_space
        modifier: none
        keycode: space
        mode: [emacs vi_insert]
        event: [
            { edit: insertstring value: " " }
            { send: menu name: goh_menu }
        ]
    })
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p goh-daemon --test nushell_menu`
Expected: PASS.

- [ ] **Step 5: Try it by hand**

In `nu -n`: `source adapters/nushell/goh.nu; goh-ui`, then type `git `, `c`, `docker run --`, `cd `. Expected: the box opens after each Space, narrows as you type, Esc closes it, and a line with no candidates opens nothing.

- [ ] **Step 6: Commit**

```bash
git add adapters/nushell/goh.nu crates/goh-daemon/tests/nushell_menu.rs
git commit -m "feat(nu): a bordered menu of goh's candidates as you type"
```

---

### Task B3: CI and the Nushell spec

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `docs/features/nushell-adapter.md`
- Modify: `docs/features/other-shells.md`, `docs/features/README.md`

- [ ] **Step 1: Install nushell in CI**

In job `test-macos`, add before `cargo test` (merge with the `brew install fish` line if the quality-gates plan already added it):

```yaml
      - run: brew install nushell
```

PowerShell is preinstalled on GitHub's macOS runners; if `pwsh` is missing in the job log, add `brew install --cask powershell` to the same step. Run `actionlint .github/workflows/ci.yml`; expected: no output.

- [ ] **Step 2: Write the spec**

`docs/features/nushell-adapter.md`:

```markdown
---
id: nushell-adapter
title: Nushell: a menu as you type
status: experimental
since: unreleased
updated: <today>
area: shells
shells: [nu]
commands: [goh init nu]
---

# Nushell: a menu as you type

In Nushell, goh adds a bordered menu next to the cursor that opens at each new word with goh's candidates and their
descriptions. In `config.nu`: `source /path/to/goh.nu`, then `goh-ui`.

## Behaviour

- Space opens the menu for the next word; typing narrows it; Down/Up move, Enter takes the highlighted candidate,
  Esc closes it. With `completions.persistent_menus` (recent Nushell), erasing keeps it open.
- Tab still completes through goh's external completer, as before (`goh-complete`).
- Your own menus and key bindings stay; goh only appends `goh_menu` and a Space binding.

## Limits

- Opens at a space, not at every letter: Nushell opens menus only from a key binding.
- No popup at the caret; the box is Nushell's own IDE menu.

## Changes

- unreleased: first release.
```

In `docs/features/other-shells.md`: remove `Nushell` from the title and the first sentence and `nu` from `shells:`, set `updated:`, and add under `## Changes`: `- unreleased: Nushell has its own spec (nushell-adapter).`

In `docs/features/README.md`, add after the `powershell-adapter` row (or the `other-shells` row if Part A did not ship):

```markdown
| [nushell-adapter](nushell-adapter.md) | shells | experimental | unreleased |
```

- [ ] **Step 3: Check and commit**

Run: `scripts/check-feature-specs.sh`
Expected: exit 0.

```bash
git add .github/workflows/ci.yml docs/features
git commit -m "docs: describe the Nushell menu; ci: install nushell"
```
