//! `cargo run --release -p goh-core --example bench`: how long the bundled specs take to load and one keystroke
//! takes to complete, held to budgets CI checks. Exits 1 over a budget.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use goh_core::{bundled_specs, Engine};

/// Lines people type, across small and very large specs (aws, gcloud, kubectl, helm).
const LINES: &[&str] = &[
    "git c", "git checkout ", "git commit --am", "docker r", "docker run -", "kubectl get ", "kubectl logs -",
    "npm run ", "cargo b", "cargo build --", "aws s3 c", "gcloud compute instances ", "brew in", "ls -", "ssh -",
    "tar -x", "terraform pl", "helm upgrade --", "gh pr cr", "sudo git ch", "git chekcout",
];

/// For a release build on the CI runner: loose enough not to flake, tight enough to catch a real slowdown.
const LOAD_BUDGET: Duration = Duration::from_millis(400);
const P95_BUDGET: Duration = Duration::from_millis(5);

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() -> ExitCode {
    let start = Instant::now();
    let specs = bundled_specs();
    let load = start.elapsed();
    let count = specs.len();
    let engine = Engine::new(specs);
    let cwd = std::env::temp_dir();
    let mut times = Vec::new();
    for _ in 0..20 {
        for line in LINES {
            let t = Instant::now();
            let _ = engine.complete_full(line, line.len(), &cwd);
            times.push(t.elapsed());
        }
    }
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!("specs {count}, load {:.1} ms (budget {} ms)", ms(load), LOAD_BUDGET.as_millis());
    println!("complete p50 {:.2} ms, p95 {:.2} ms (budget {} ms), max {:.2} ms", ms(at(0.5)), ms(at(0.95)), P95_BUDGET.as_millis(), ms(at(1.0)));
    if load > LOAD_BUDGET || at(0.95) > P95_BUDGET {
        eprintln!("over budget");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
