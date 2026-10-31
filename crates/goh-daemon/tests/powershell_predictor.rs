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
         # default(SuggestionPackage), what the predictor returns for nothing, has null entries: @($null).Count is 1.
         'COUNT ' + $(if ($pkg.SuggestionEntries) {{ $pkg.SuggestionEntries.Count }} else {{ 0 }})",
        adapter().display()
    );
    let out = fx.command(pwsh.to_str().unwrap(), &["-NoProfile", "-NonInteractive", "-Command", &script], &[]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("COUNT 0"), "{}", String::from_utf8_lossy(&out.stderr));
}
