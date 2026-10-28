//! Package names for `brew`, `cargo uninstall`, `pip` and `uv tool`, read from what the package managers keep on
//! disk: Homebrew's Cellar, Caskroom and its local copy of the formula API, cargo's install records, `site-packages`,
//! uv's tool directory. Plain file and directory reads, bounded in size; none of the tools is started (`brew` alone
//! takes longer than a keystroke can wait).

use std::path::{Path, PathBuf};

use goh_core::spec::{Provider, Suggestion};

use crate::project::described;

/// Homebrew alone has about 17000 formulae and casks.
const MAX_NAMES: usize = 50_000;
const MAX_FILE: u64 = 4 << 20;

pub fn reads(provider: Provider) -> bool {
    matches!(
        provider,
        Provider::BrewInstalled | Provider::BrewAvailable | Provider::CargoInstalled | Provider::PipPackages | Provider::UvTools
    )
}

/// Only `PipPackages` depends on the working directory (a project's virtualenv); the key of the cache follows it.
pub fn reads_cwd(provider: Provider) -> bool {
    provider == Provider::PipPackages
}

pub fn values(provider: Provider, home: &Path, cwd: &Path) -> Vec<Suggestion> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    match provider {
        Provider::BrewInstalled => brew_prefix().map_or_else(Vec::new, |p| brew_installed(&p)),
        Provider::BrewAvailable => {
            let cache = var("HOMEBREW_CACHE").unwrap_or_else(|| {
                if cfg!(target_os = "macos") {
                    home.join("Library/Caches/Homebrew")
                } else {
                    var("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")).join("Homebrew")
                }
            });
            brew_available(brew_prefix().as_deref(), &cache)
        }
        Provider::CargoInstalled => cargo_installed(&var("CARGO_HOME").unwrap_or_else(|| home.join(".cargo"))),
        Provider::PipPackages => pip_packages(&site_dirs(home, cwd, brew_prefix().as_deref())),
        Provider::UvTools => {
            let data = var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
            uv_tools(&var("UV_TOOL_DIR").unwrap_or_else(|| data.join("uv/tools")))
        }
        _ => vec![],
    }
}

fn read_text(path: &Path) -> Option<String> {
    (std::fs::metadata(path).ok()?.len() <= MAX_FILE).then(|| std::fs::read_to_string(path).ok()).flatten()
}

/// Names of the folders (and links to folders) in `dir`, without hidden ones, sorted.
fn dir_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

// ---- Homebrew ---------------------------------------------------------------------------------

/// Where this machine's Homebrew lives, without asking `brew`.
fn brew_prefix() -> Option<PathBuf> {
    let set = std::env::var_os("HOMEBREW_PREFIX").filter(|v| !v.is_empty()).map(PathBuf::from);
    set.into_iter()
        .chain(["/opt/homebrew", "/usr/local", "/home/linuxbrew/.linuxbrew"].map(PathBuf::from))
        .find(|p| p.join("Cellar").is_dir() || p.join("Caskroom").is_dir())
}

/// What is installed: one folder per formula in the Cellar and per cask in the Caskroom.
fn brew_installed(prefix: &Path) -> Vec<Suggestion> {
    let kind = |sub: &str, what: &'static str| dir_names(&prefix.join(sub)).into_iter().map(move |n| (n, what));
    described(kind("Cellar", "installed formula").chain(kind("Caskroom", "installed cask")), MAX_NAMES)
}

/// Names Homebrew can install: the lists it keeps in its cache (`brew update` refreshes them), plus the formulae and
/// casks of the taps you added, whose `.rb` files sit in `Formula/` and `Casks/` (one folder deeper when sharded).
fn brew_available(prefix: Option<&Path>, cache: &Path) -> Vec<Suggestion> {
    let lines = |file: &str, what: &'static str| {
        read_text(&cache.join("api").join(file)).unwrap_or_default().lines().map(|l| (l.trim().to_string(), what)).collect::<Vec<_>>()
    };
    let mut all = lines("formula_names.txt", "formula");
    all.extend(lines("cask_names.txt", "cask"));
    for tap in prefix.map(|p| tap_dirs(&p.join("Library/Taps"))).into_iter().flatten() {
        for (sub, what) in [("Formula", "tap formula"), ("HomebrewFormula", "tap formula"), ("Casks", "tap cask")] {
            all.extend(rb_names(&tap.join(sub), 2).into_iter().map(|n| (n, what)));
        }
    }
    described(all, MAX_NAMES)
}

/// `Taps/<user>/<homebrew-repo>`
fn tap_dirs(taps: &Path) -> Vec<PathBuf> {
    dir_names(taps).iter().flat_map(|user| dir_names(&taps.join(user)).into_iter().map(move |repo| taps.join(user).join(repo))).collect()
}

fn rb_names(dir: &Path, depth: usize) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(name) = e.file_name().into_string() else { continue };
        if let Some(stem) = name.strip_suffix(".rb") {
            out.push(stem.to_string());
        } else if depth > 0 && e.path().is_dir() {
            out.extend(rb_names(&e.path(), depth - 1));
        }
    }
    out.sort();
    out
}

// ---- cargo, uv, pip ---------------------------------------------------------------------------

/// Crates `cargo install` put there, from the records cargo keeps in `$CARGO_HOME` (`.crates2.json`, older
/// versions `.crates.toml`). Each key reads `name version (source)`.
fn cargo_installed(cargo_home: &Path) -> Vec<Suggestion> {
    let keys: Vec<String> = match read_text(&cargo_home.join(".crates2.json")).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        Some(json) => json.get("installs").and_then(|v| v.as_object()).map(|o| o.keys().cloned().collect()).unwrap_or_default(),
        None => read_text(&cargo_home.join(".crates.toml"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.strip_prefix('"')?.split_once('"').map(|(k, _)| k.to_string()))
            .collect(),
    };
    let mut crates: Vec<(String, String)> = keys
        .iter()
        .filter_map(|k| {
            let mut words = k.split_whitespace();
            Some((words.next()?.to_string(), words.next().unwrap_or("installed").to_string()))
        })
        .collect();
    crates.sort();
    described(crates, MAX_NAMES)
}

/// `uv tool install` gives each tool a folder of its own.
fn uv_tools(tools: &Path) -> Vec<Suggestion> {
    described(dir_names(tools).into_iter().map(|n| (n, "uv tool")), MAX_NAMES)
}

/// The `site-packages` folders `pip` would work on: the virtualenv in this project (or above it) if there is one,
/// else the user's and the system's.
fn site_dirs(home: &Path, cwd: &Path, brew: Option<&Path>) -> Vec<PathBuf> {
    // `lib/python3.12/site-packages`, and `dist-packages` where Debian puts it.
    let in_lib = |lib: &Path| -> Vec<PathBuf> {
        dir_names(lib)
            .into_iter()
            .filter(|n| n.starts_with("python"))
            .flat_map(|n| ["site-packages", "dist-packages"].map(|s| lib.join(&n).join(s)))
            .filter(|p| p.is_dir())
            .collect()
    };
    for dir in cwd.ancestors() {
        for venv in [".venv", "venv", "env"] {
            let found = in_lib(&dir.join(venv).join("lib"));
            if !found.is_empty() {
                return found;
            }
        }
    }
    let libs = [home.join(".local/lib")].into_iter().chain(brew.map(|p| p.join("lib"))).chain(["/usr/local/lib", "/usr/lib"].map(PathBuf::from));
    let mut out: Vec<PathBuf> = libs.flat_map(|lib| in_lib(&lib)).collect();
    // macOS user site: `~/Library/Python/3.9/lib/python/site-packages`.
    let user = home.join("Library/Python");
    out.extend(dir_names(&user).into_iter().map(|v| user.join(v).join("lib/python/site-packages")).filter(|p| p.is_dir()));
    out
}

/// Installed distributions: `name-1.2.3.dist-info` (or `.egg-info`) folders. Normalised names use `_`; `pip` takes either.
fn pip_packages(site_dirs: &[PathBuf]) -> Vec<Suggestion> {
    let mut found = Vec::new();
    for dir in site_dirs {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let Ok(entry) = e.file_name().into_string() else { continue };
            let Some(stem) = entry.strip_suffix(".dist-info").or_else(|| entry.strip_suffix(".egg-info")) else { continue };
            let (name, rest) = stem.split_once('-').unwrap_or((stem, ""));
            // `.egg-info` folders go on with `-py3.12`.
            let version = rest.split('-').next().filter(|v| !v.is_empty()).unwrap_or("installed");
            found.push((name.to_string(), version.to_string()));
        }
    }
    found.sort();
    described(found, MAX_NAMES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[Suggestion]) -> Vec<&str> {
        v.iter().map(|s| s.name.as_str()).collect()
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn brew_installed_lists_cellar_and_caskroom_folders() {
        let prefix = tempfile::tempdir().unwrap();
        let p = prefix.path();
        for d in ["Cellar/wget/1.24", "Cellar/python@3.13/3.13.1", "Cellar/.keepme", "Caskroom/iterm2/3.5", "Caskroom/.metadata"] {
            std::fs::create_dir_all(p.join(d)).unwrap();
        }
        touch(&p.join("Cellar/stray-file"));
        let got = brew_installed(p);
        assert_eq!(names(&got), ["python@3.13", "wget", "iterm2"]);
        assert_eq!((got[0].description.as_deref(), got[2].description.as_deref()), (Some("installed formula"), Some("installed cask")));
    }

    #[test]
    fn brew_installed_is_empty_without_a_homebrew() {
        assert!(brew_installed(Path::new("/nonexistent/homebrew")).is_empty());
    }

    #[test]
    fn brew_available_reads_the_api_cache_and_the_taps() {
        let root = tempfile::tempdir().unwrap();
        let (prefix, cache) = (root.path().join("prefix"), root.path().join("cache"));
        std::fs::create_dir_all(cache.join("api")).unwrap();
        std::fs::write(cache.join("api/formula_names.txt"), "wget\njq\n\nbad name\n-rf\n").unwrap();
        std::fs::write(cache.join("api/cask_names.txt"), "iterm2\nwget\n").unwrap();
        touch(&prefix.join("Library/Taps/me/homebrew-goh/Formula/goh.rb"));
        touch(&prefix.join("Library/Taps/me/homebrew-goh/Formula/g/gohd.rb"));
        touch(&prefix.join("Library/Taps/me/homebrew-goh/Casks/goh-app.rb"));
        touch(&prefix.join("Library/Taps/me/homebrew-goh/README.md"));
        let got = brew_available(Some(&prefix), &cache);
        assert_eq!(names(&got), ["wget", "jq", "iterm2", "goh", "gohd", "goh-app"]);
        assert_eq!(got[0].description.as_deref(), Some("formula"), "the first of a repeated name stays");
        assert_eq!(got[5].description.as_deref(), Some("tap cask"));
    }

    #[test]
    fn brew_available_fails_open_without_cache_or_taps() {
        let none = Path::new("/nonexistent/cache");
        assert!(brew_available(None, none).is_empty());
        assert!(brew_available(Some(none), none).is_empty());
    }

    #[test]
    fn oversized_or_unreadable_name_lists_are_skipped() {
        let cache = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(cache.path().join("api")).unwrap();
        std::fs::write(cache.path().join("api/formula_names.txt"), vec![b'a'; (MAX_FILE + 1) as usize]).unwrap();
        std::fs::write(cache.path().join("api/cask_names.txt"), [0xff, 0xfe, b'\n']).unwrap(); // not UTF-8
        assert!(brew_available(None, cache.path()).is_empty());
    }

    #[test]
    fn cargo_installed_reads_the_install_records() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".crates2.json"), r#"{"installs":{"ripgrep 14.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["rg"]},"goh 1.1.3 (path+file:///x)":{}}}"#).unwrap();
        let got = cargo_installed(home.path());
        assert_eq!(names(&got), ["goh", "ripgrep"]);
        assert_eq!(got[1].description.as_deref(), Some("14.1.0"));

        let old = tempfile::tempdir().unwrap();
        std::fs::write(old.path().join(".crates.toml"), "[v1]\n\"fd-find 10.0.0 (registry+https://x)\" = [\"fd\"]\n").unwrap();
        assert_eq!(names(&cargo_installed(old.path())), ["fd-find"]);

        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join(".crates2.json"), "{not json").unwrap();
        assert!(cargo_installed(broken.path()).is_empty(), "a broken record fails open");
        assert!(cargo_installed(Path::new("/nonexistent")).is_empty());
    }

    #[test]
    fn uv_tools_are_the_folders_of_the_tool_directory() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["ruff", "httpie", ".hidden"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        assert_eq!(names(&uv_tools(dir.path())), ["httpie", "ruff"]);
        assert!(uv_tools(Path::new("/nonexistent")).is_empty());
    }

    #[test]
    fn pip_packages_prefer_the_projects_virtualenv() {
        let root = tempfile::tempdir().unwrap();
        let (home, project) = (root.path().join("home"), root.path().join("proj"));
        let user = home.join(".local/lib/python3.12/site-packages");
        let venv = project.join(".venv/lib/python3.12/site-packages");
        let mac = home.join("Library/Python/3.9/lib/python/site-packages");
        for (dir, entries) in [
            (&user, &["black-24.1.0.dist-info", "attrs-23.1.0.dist-info"][..]),
            (&venv, &["requests-2.31.0.dist-info", "typing_extensions-4.9.0.dist-info", "oldpkg-0.1-py3.12.egg-info", "requests"][..]),
            (&mac, &["six-1.16.0.dist-info"][..]),
        ] {
            for e in entries {
                std::fs::create_dir_all(dir.join(e)).unwrap();
            }
        }
        std::fs::create_dir_all(project.join("src/deep")).unwrap();
        let got = pip_packages(&site_dirs(&home, &project.join("src/deep"), None));
        assert_eq!(names(&got), ["oldpkg", "requests", "typing_extensions"]);
        assert_eq!((got[0].description.as_deref(), got[1].description.as_deref()), (Some("0.1"), Some("2.31.0")));

        // Outside a project: the user site, macOS style included.
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        assert_eq!(names(&pip_packages(&site_dirs(&home, &elsewhere, None))), ["attrs", "black", "six"]);
        assert!(pip_packages(&[PathBuf::from("/nonexistent")]).is_empty());
    }

    #[test]
    fn only_pip_packages_follow_the_working_directory() {
        assert!(reads(Provider::BrewInstalled) && !reads(Provider::GitBranches));
        assert!(reads_cwd(Provider::PipPackages) && !reads_cwd(Provider::BrewInstalled));
    }
}
