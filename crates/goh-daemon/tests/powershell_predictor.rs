//! adapters/powershell/goh.ps1's predictor in a real pwsh, against a test daemon: the suggestions PSReadLine would
//! show in its list. No terminal needed. Skipped (with a note) when pwsh 7.2+ or a UTF-8 locale is missing.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::{utf8_locale, Fixture};

fn pwsh() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("pwsh")).find(|f| f.is_file()))
}

fn adapter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/powershell/goh.ps1")
}

/// Runs `body` in a pwsh that has dot-sourced the adapter first (`. adapter`), in the fixture's clean environment.
fn run(fx: &Fixture, pwsh: &Path, body: &str, env: &[(&str, &str)]) -> Output {
    let script = format!(". '{}'\n{body}", adapter().display());
    fx.command(pwsh.to_str().unwrap(), &["-NoProfile", "-NonInteractive", "-Command", &script], env).output().unwrap()
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Asks the predictor for `git chec` the way PSReadLine would and prints one `suggestion<TAB>tooltip` line each.
const ASK: &str = "
    if (-not (Import-GohPredictor)) { 'NO PREDICTOR'; exit 1 }
    $ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git chec')
    $client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('test', 'Terminal')
    $pkg = ([Goh.Predictor]::new()).GetSuggestion($client, $ctx, [Threading.CancellationToken]::None)
    foreach ($s in $pkg.SuggestionEntries) { \"$($s.SuggestionText)`t$($s.ToolTip)\" }";

fn setup() -> Option<(PathBuf, String)> {
    Some((pwsh()?, utf8_locale()?))
}

fn assert_whole_lines(out: &Output) {
    let text = text(out);
    assert!(out.status.success(), "{text}");
    let first = text.lines().next().unwrap_or_default();
    assert!(first.starts_with("git checkout"), "the whole line, with the word completed: {text}");
    assert!(first.contains('\t') && !first.ends_with('\t'), "with the description as its tooltip: {text}");
}

#[test]
fn the_predictor_suggests_whole_lines_from_goh() {
    let Some((pwsh, locale)) = setup() else {
        eprintln!("skipping: needs pwsh and a UTF-8 locale");
        return;
    };
    let fx = Fixture::new(locale, true);
    assert_whole_lines(&run(&fx, &pwsh, ASK, &[]));
}

#[test]
fn a_tab_completion_does_not_silence_the_predictor() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, true);
    // `goh menu --session ps-$PID` is what the Tab completer runs; its time-based generation used to leave every later
    // predictor request (same session, generation counting from 1) Stale.
    let body = format!(
        "& goh menu --session \"ps-$PID\" --cwd \"$PWD\" --limit 5 --cursor-byte 8 --buffer 'git chec' | Out-Null\n{ASK}"
    );
    assert_whole_lines(&run(&fx, &pwsh, &body, &[]));
}

#[test]
fn no_daemon_means_no_suggestions_and_no_error() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, false);
    let body = "
        if (-not (Import-GohPredictor)) { exit 1 }
        [Goh.State]::Socket = '/nonexistent/gohd.sock'
        $ctx = [System.Management.Automation.Subsystem.Prediction.PredictionContext]::Create('git chec')
        $client = [System.Management.Automation.Subsystem.Prediction.PredictionClient]::new('test', 'Terminal')
        $pkg = ([Goh.Predictor]::new()).GetSuggestion($client, $ctx, [Threading.CancellationToken]::None)
        # default(SuggestionPackage), what the predictor returns for nothing, has null entries: @($null).Count is 1.
        'COUNT ' + $(if ($pkg.SuggestionEntries) { $pkg.SuggestionEntries.Count } else { 0 })";
    let out = run(&fx, &pwsh, body, &[]);
    assert!(text(&out).contains("COUNT 0"), "{}", text(&out));
}

#[test]
fn a_corrupt_cached_dll_is_replaced_and_the_predictor_still_loads() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, true);
    assert_whole_lines(&run(&fx, &pwsh, ASK, &[])); // compiles into the fixture's cache
    let cache = fx.path(".cache/goh/powershell");
    let dlls: Vec<PathBuf> = fs::read_dir(&cache).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(dlls.len(), 1, "only the compiled dll, no leftover temporary files: {dlls:?}");
    let good = fs::metadata(&dlls[0]).unwrap().len();
    fs::write(&dlls[0], b"not a dll").unwrap();
    assert_whole_lines(&run(&fx, &pwsh, ASK, &[]));
    assert_eq!(fs::metadata(&dlls[0]).unwrap().len(), good, "the garbage was replaced by a fresh compile");
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 1, "still no leftovers");
}

/// A `goh` that only notes in `marker` that it was asked to `start`.
fn fake_goh(fx: &Fixture) -> (PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;
    let bin = fx.path("fakebin");
    fs::create_dir(&bin).unwrap();
    let marker = fx.path("started");
    let goh = bin.join("goh");
    fs::write(&goh, format!("#!/bin/sh\n[ \"$1\" = start ] && touch '{}'\nexit 0\n", marker.display())).unwrap();
    fs::set_permissions(&goh, fs::Permissions::from_mode(0o755)).unwrap();
    (marker, format!("{}:/usr/bin:/bin", bin.display()))
}

/// What a shell start does with PSReadLine loaded: the dot-source runs `Enable-GohPredictor`.
const WITH_PSREADLINE: &str =
    "Import-Module PSReadLine\nSet-PSReadLineOption -PredictionSource History -PredictionViewStyle InlineView\n";

fn dot_source_with_psreadline(fx: &Fixture, pwsh: &Path, body: &str, env: &[(&str, &str)]) -> Output {
    let script = format!("{WITH_PSREADLINE}. '{}'\n{body}", adapter().display());
    fx.command(pwsh.to_str().unwrap(), &["-NoProfile", "-NonInteractive", "-Command", &script], env).output().unwrap()
}

const LOADED: &str = "$o = Get-PSReadLineOption; 'LOADED ' + [bool]('Goh.Predictor' -as [type]) + ' ' + $o.PredictionSource + ' ' + $o.PredictionViewStyle";

#[test]
fn live_off_does_nothing_and_live_on_widens_the_prediction_source() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, true);
    let off = dot_source_with_psreadline(&fx, &pwsh, LOADED, &[("GOH_LIVE", "off")]);
    let off = text(&off);
    assert!(off.contains("LOADED False History InlineView"), "GOH_LIVE=off leaves PSReadLine alone: {off}");
    assert!(!fx.path(".cache/goh/powershell").exists(), "and compiles nothing: {off}");
    let on = text(&dot_source_with_psreadline(&fx, &pwsh, LOADED, &[("GOH_LIVE", "on")]));
    assert!(on.contains("LOADED True HistoryAndPlugin ListView"), "the predictor is loaded, its source added: {on}");
}

#[test]
fn display_live_false_in_the_config_file_turns_the_predictor_off() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, true);
    let config = fx.path("config.json");
    fs::write(&config, r#"{"display":{"live":false}}"#).unwrap();
    let script = format!("{WITH_PSREADLINE}. '{}'\n{LOADED}", adapter().display());
    let args = ["-NoProfile", "-NonInteractive", "-Command", script.as_str()];
    // The fixture sets GOH_LIVE=off for the Tab tests; here only the config file says so.
    let mut cmd = fx.command(pwsh.to_str().unwrap(), &args, &[("GOH_CONFIG", config.to_str().unwrap())]);
    let t = text(&cmd.env_remove("GOH_LIVE").output().unwrap());
    assert!(t.contains("LOADED False History InlineView"), "display.live: false leaves PSReadLine alone: {t}");
    assert!(!fx.path(".cache/goh/powershell").exists(), "and compiles nothing: {t}");
    // The environment variable still wins over the file.
    let env = [("GOH_CONFIG", config.to_str().unwrap()), ("GOH_LIVE", "on")];
    let on = text(&dot_source_with_psreadline(&fx, &pwsh, LOADED, &env));
    assert!(on.contains("LOADED True HistoryAndPlugin ListView"), "{on}");
}

#[test]
fn the_daemon_is_started_only_when_its_socket_is_missing() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, true);
    let (marker, path) = fake_goh(&fx);
    let up = dot_source_with_psreadline(&fx, &pwsh, "", &[("GOH_LIVE", "on"), ("PATH", &path)]);
    assert!(up.status.success(), "{}", text(&up));
    assert!(!marker.exists(), "the socket is there: goh start must not run");
    let down = dot_source_with_psreadline(
        &fx,
        &pwsh,
        "",
        &[("GOH_LIVE", "on"), ("PATH", &path), ("GOH_SOCKET", fx.path("nowhere/s").to_str().unwrap())],
    );
    assert!(down.status.success(), "{}", text(&down));
    assert!(marker.exists(), "no socket: goh start runs");
}

#[test]
fn a_missing_goh_never_breaks_profile_load() {
    let Some((pwsh, locale)) = setup() else { return };
    let fx = Fixture::new(locale, false);
    let body = "$ErrorActionPreference = 'Stop'\ntry { Enable-GohPredictor; 'DONE' } catch { 'THREW ' + $_ }";
    let out = dot_source_with_psreadline(
        &fx,
        &pwsh,
        body,
        &[("GOH_LIVE", "on"), ("PATH", "/usr/bin:/bin"), ("GOH_SOCKET", fx.path("nowhere/s").to_str().unwrap())],
    );
    let t = text(&out);
    assert!(t.contains("DONE") && !t.contains("THREW"), "{t}");
}
