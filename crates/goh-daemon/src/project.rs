//! Suggestions that depend on the current directory's project: git branches, remotes and aliases,
//! `package.json`, `pyproject.toml` and `deno.json` scripts, Makefile targets. Plain file reads (no `git`,
//! `npm` or `make` is run), bounded in size.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use goh_core::spec::{Provider, Suggestion};

const MAX_NAMES: usize = 2000;
const MAX_DEPTH: usize = 8;
const MAX_FILE: u64 = 1 << 20;

fn suggestion(name: String, description: Option<String>) -> Suggestion {
    Suggestion { name, description, insert_value: None }
}

fn read_small(path: &Path) -> Option<String> {
    (std::fs::metadata(path).ok()?.len() <= MAX_FILE).then(|| std::fs::read_to_string(path).ok()).flatten()
}

pub fn values(provider: Provider, cwd: &Path) -> Vec<Suggestion> {
    match provider {
        Provider::GitBranches => git_branches(cwd),
        Provider::GitRemotes => git_remotes(cwd),
        Provider::GitTags => git_tags(cwd),
        Provider::GitStashes => git_stashes(cwd),
        Provider::PackageScripts => package_scripts(cwd),
        Provider::MakeTargets => make_targets(cwd),
        Provider::JustRecipes => just_recipes(cwd),
        Provider::TaskfileTasks => taskfile_tasks(cwd),
        Provider::ComposeServices => compose_services(cwd),
        Provider::CargoBins => cargo_bins(cwd),
        Provider::CargoExamples => cargo_examples(cwd),
        Provider::CargoFeatures => cargo_features(cwd),
        Provider::WorkspacePackages => workspace_packages(cwd),
        Provider::PackageDependencies => package_dependencies(cwd),
        Provider::CargoDependencies => cargo_dependencies(cwd),
        Provider::GitAliases => git_aliases(cwd, &std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()),
        Provider::PythonScripts => python_scripts(cwd),
        Provider::DenoTasks => deno_tasks(cwd),
        Provider::NodeBins => node_bins(cwd),
        _ => vec![],
    }
}

/// Providers answered by reading project files here (the rest query a program or the user's own files).
pub fn reads_project(provider: Provider) -> bool {
    matches!(
        provider,
        Provider::GitBranches | Provider::GitRemotes | Provider::GitTags | Provider::GitStashes | Provider::PackageScripts
            | Provider::MakeTargets | Provider::JustRecipes | Provider::TaskfileTasks | Provider::ComposeServices
            | Provider::CargoBins | Provider::CargoExamples | Provider::CargoFeatures | Provider::WorkspacePackages
            | Provider::PackageDependencies | Provider::CargoDependencies | Provider::GitAliases | Provider::PythonScripts
            | Provider::DenoTasks | Provider::NodeBins
    )
}

/// A name that is safe to offer as a plain word: no option look-alikes, spaces or control characters.
pub(crate) fn plain(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && !name.starts_with('-') && !name.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Named things with a description each: only plain words, the first of a repeated name, at most `cap`.
pub(crate) fn described<D: Into<String>>(list: impl IntoIterator<Item = (String, D)>, cap: usize) -> Vec<Suggestion> {
    let mut seen = HashSet::new();
    list.into_iter().filter(|(n, _)| plain(n) && seen.insert(n.clone())).take(cap).map(|(n, d)| suggestion(n, Some(d.into()))).collect()
}

// ---- git --------------------------------------------------------------------------------------

struct Git {
    /// This checkout's own directory (`HEAD` lives here; differs from `common` in a linked worktree).
    dir: PathBuf,
    /// Where refs and config live.
    common: PathBuf,
}

fn find_git(cwd: &Path) -> Option<Git> {
    for dir in cwd.ancestors() {
        let dot = dir.join(".git");
        if dot.is_dir() {
            return Some(Git { dir: dot.clone(), common: dot });
        }
        // A worktree or submodule: `.git` is a file saying `gitdir: <path>`.
        if let Some(text) = dot.is_file().then(|| read_small(&dot)).flatten() {
            let gitdir = dir.join(text.trim().strip_prefix("gitdir:")?.trim());
            let common = read_small(&gitdir.join("commondir")).map_or_else(|| gitdir.clone(), |c| gitdir.join(c.trim()));
            return Some(Git { dir: gitdir, common });
        }
    }
    None
}

/// Ref names under `base` (loose files), as paths relative to it.
fn loose_refs(base: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for e in read.flatten() {
            let Ok(name) = e.file_name().into_string() else { continue };
            let full = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            match e.file_type() {
                Ok(t) if t.is_dir() && depth < MAX_DEPTH => walk(&e.path(), &full, depth + 1, out),
                Ok(t) if t.is_file() && !name.ends_with(".lock") => out.push(full),
                _ => {}
            }
            if out.len() >= MAX_NAMES {
                return;
            }
        }
    }
    let mut out = Vec::new();
    walk(base, "", 0, &mut out);
    out
}

/// Ref names from `packed-refs` that start with `prefix` (which ends in `/`), prefix removed.
fn packed_refs(text: &str, prefix: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.split_once(' ')?.1.strip_prefix(prefix)).map(str::to_string).collect()
}

fn git_branches(cwd: &Path) -> Vec<Suggestion> {
    let Some(git) = find_git(cwd) else { return vec![] };
    let packed = read_small(&git.common.join("packed-refs")).unwrap_or_default();
    let current = read_small(&git.dir.join("HEAD")).and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_string));

    let mut local = loose_refs(&git.common.join("refs/heads"));
    local.extend(packed_refs(&packed, "refs/heads/"));
    let mut seen = HashSet::new();
    local.retain(|n| seen.insert(n.clone()));
    // Current branch first, then the rest alphabetically: stable, and what you most likely want to leave or compare.
    local.sort_by(|a, b| (current.as_deref() != Some(a)).cmp(&(current.as_deref() != Some(b))).then_with(|| a.cmp(b)));
    let mut out: Vec<Suggestion> = local
        .into_iter()
        .map(|n| {
            let d = if current.as_deref() == Some(n.as_str()) { "current branch" } else { "branch" };
            suggestion(n, Some(d.into()))
        })
        .collect();

    // Remote branches you have no local branch for yet: `git checkout name` creates the tracking branch.
    let mut remote = loose_refs(&git.common.join("refs/remotes"));
    remote.extend(packed_refs(&packed, "refs/remotes/"));
    for r in remote {
        let Some((origin, name)) = r.split_once('/') else { continue };
        if name != "HEAD" && seen.insert(name.to_string()) {
            out.push(suggestion(name.to_string(), Some(format!("remote branch ({origin})"))));
        }
    }
    out.retain(|s| !s.name.starts_with('-') && !s.name.chars().any(|c| c.is_control() || c.is_whitespace()));
    out.truncate(MAX_NAMES);
    out
}

/// `[remote "name"]` sections of a git config, with their `url`.
pub fn remotes_in(config: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut in_remote = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_remote = false;
            if let Some(name) = line.strip_prefix("[remote \"").and_then(|r| r.strip_suffix("\"]")) {
                out.push((name.to_string(), None));
                in_remote = true;
            }
        } else if in_remote {
            if let Some(url) = line.strip_prefix("url").and_then(|r| r.trim_start().strip_prefix('=')) {
                if let Some(last) = out.last_mut().filter(|l| l.1.is_none()) {
                    last.1 = Some(url.trim().to_string());
                }
            }
        }
    }
    out
}

fn git_remotes(cwd: &Path) -> Vec<Suggestion> {
    let Some(git) = find_git(cwd) else { return vec![] };
    let config = read_small(&git.common.join("config")).unwrap_or_default();
    remotes_in(&config).into_iter().map(|(n, url)| suggestion(n, url)).collect()
}

/// `[alias]` entries of a git config: `co = checkout -b`. Aliases that run a shell command (`!...`) are not
/// git subcommands and are left out.
pub fn aliases_in(config: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_alias = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_alias = line.trim_start_matches('[').trim_end_matches(']').trim().eq_ignore_ascii_case("alias");
            continue;
        }
        let Some((name, value)) = line.split_once('=').filter(|_| in_alias && !line.starts_with(['#', ';'])) else { continue };
        let value = value.trim().trim_matches('"').trim();
        if !value.is_empty() && !value.starts_with('!') && !value.chars().any(char::is_control) {
            out.push((name.trim().to_string(), value.to_string()));
        }
    }
    out
}

/// Your aliases (`~/.gitconfig`, `$XDG_CONFIG_HOME/git/config`) and the repository's, which win.
fn git_aliases(cwd: &Path, home: &Path) -> Vec<Suggestion> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from);
    let mut files = vec![xdg.join("git/config"), home.join(".gitconfig")];
    if let Some(git) = find_git(cwd) {
        files.insert(0, git.common.join("config"));
    }
    described(files.iter().filter_map(|f| read_small(f)).flat_map(|t| aliases_in(&t)), MAX_NAMES)
}

/// Digit runs compare as numbers, so `v1.10` sorts after `v1.9`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let chunks = |s: &str| {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    };
    for (x, y) in chunks(a).iter().zip(chunks(b).iter()) {
        let ord = match (x.0 && y.0, x.1.trim_start_matches('0').len().cmp(&y.1.trim_start_matches('0').len())) {
            (true, std::cmp::Ordering::Equal) => x.1.trim_start_matches('0').cmp(y.1.trim_start_matches('0')),
            (true, other) => other,
            _ => x.1.cmp(&y.1),
        };
        if ord.is_ne() {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// Tags, newest version first.
fn git_tags(cwd: &Path) -> Vec<Suggestion> {
    let Some(git) = find_git(cwd) else { return vec![] };
    let packed = read_small(&git.common.join("packed-refs")).unwrap_or_default();
    let mut tags = loose_refs(&git.common.join("refs/tags"));
    tags.extend(packed_refs(&packed, "refs/tags/"));
    let mut seen = HashSet::new();
    tags.retain(|t| plain(t) && seen.insert(t.clone()));
    tags.sort_by(|a, b| natural_cmp(b, a));
    tags.truncate(MAX_NAMES);
    tags.into_iter().map(|t| suggestion(t, Some("tag".into()))).collect()
}

/// `stash@{0}` is the newest: the reflog of `refs/stash` lists them oldest first.
fn git_stashes(cwd: &Path) -> Vec<Suggestion> {
    let Some(git) = find_git(cwd) else { return vec![] };
    let log = read_small(&git.common.join("logs/refs/stash")).unwrap_or_default();
    let messages: Vec<&str> = log.lines().map(|l| l.split_once('\t').map_or("", |(_, m)| m)).collect();
    messages
        .iter()
        .rev()
        .enumerate()
        .take(100)
        .map(|(i, m)| Suggestion {
            name: format!("stash@{{{i}}}"),
            description: Some(m.chars().filter(|c| !c.is_control()).take(80).collect()),
            insert_value: Some(format!("stash@{{{i}}} ")),
        })
        .collect()
}

// ---- project task files -------------------------------------------------------------------------

fn nearest(cwd: &Path, names: &[&str]) -> Option<String> {
    cwd.ancestors().find_map(|d| names.iter().find_map(|n| read_small(&d.join(n))))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// The entries of a top-level YAML mapping: `key:` at column 0, then each child name at the first deeper
/// indent, with the lines that belong to it. A reader for the plain layout of compose and task files, not a
/// YAML parser: flow style, anchors and merged keys are left out.
fn yaml_children<'a>(text: &'a str, key: &str) -> Vec<(String, Vec<&'a str>)> {
    let mut out: Vec<(String, Vec<&'a str>)> = Vec::new();
    let mut inside = false;
    let mut child_indent = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            inside = trimmed.strip_prefix(key).is_some_and(|r| r.trim_start().starts_with(':'));
            child_indent = None;
            continue;
        }
        if !inside {
            continue;
        }
        let level = *child_indent.get_or_insert(indent);
        if indent == level {
            let name = trimmed.split_once(':').map(|(n, _)| n.trim().trim_matches(|c| c == '"' || c == '\''));
            if let Some(name) = name.filter(|n| plain(n) && !n.starts_with('<')) {
                out.push((name.to_string(), Vec::new()));
            }
        } else if indent > level {
            if let Some(last) = out.last_mut() {
                last.1.push(trimmed);
            }
        }
    }
    out
}

pub fn compose_services_in(text: &str) -> Vec<(String, Option<String>)> {
    yaml_children(text, "services")
        .into_iter()
        .map(|(name, body)| {
            let image = body.iter().find_map(|l| l.strip_prefix("image:")).map(|i| i.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
            (name, image.filter(|i| !i.is_empty() && !i.chars().any(char::is_control)))
        })
        .collect()
}

fn compose_services(cwd: &Path) -> Vec<Suggestion> {
    let files = ["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];
    let Some(text) = nearest(cwd, &files) else { return vec![] };
    compose_services_in(&text).into_iter().take(MAX_NAMES).map(|(n, image)| suggestion(n, image)).collect()
}

pub fn taskfile_tasks_in(text: &str) -> Vec<(String, Option<String>)> {
    yaml_children(text, "tasks")
        .into_iter()
        .map(|(name, body)| {
            let desc = body.iter().find_map(|l| l.strip_prefix("desc:")).map(|d| d.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
            (name, desc.filter(|d| !d.is_empty()))
        })
        .collect()
}

fn taskfile_tasks(cwd: &Path) -> Vec<Suggestion> {
    let files = ["Taskfile.yml", "Taskfile.yaml", "taskfile.yml", "taskfile.yaml"];
    let Some(text) = nearest(cwd, &files) else { return vec![] };
    taskfile_tasks_in(&text).into_iter().take(MAX_NAMES).map(|(n, d)| suggestion(n, d)).collect()
}

/// Recipes of a justfile: `name params:` at column 0, the `# comment` above it as the description. Private
/// recipes (`_name`) and assignments, settings and imports are not recipes.
pub fn just_recipes_in(text: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut note: Option<String> = None;
    for line in text.lines() {
        if let Some(comment) = line.strip_prefix('#') {
            note = Some(comment.trim().to_string());
            continue;
        }
        if line.starts_with([' ', '\t']) || line.trim().is_empty() {
            if line.trim().is_empty() {
                note = None;
            }
            continue;
        }
        let head = line.split_once(':').map_or(line, |(h, _)| h);
        let is_recipe = line.contains(':') && !line.contains(":=") && !line.starts_with("[");
        let first = head.split_whitespace().next().unwrap_or("").trim_start_matches('@');
        let keyword = matches!(first, "set" | "export" | "alias" | "import" | "mod" | "unexport");
        if is_recipe && !keyword && !first.starts_with('_') && first.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) && !first.is_empty() {
            out.push((first.to_string(), note.take().filter(|n| !n.is_empty())));
        }
        note = None;
    }
    out
}

fn just_recipes(cwd: &Path) -> Vec<Suggestion> {
    let Some(text) = nearest(cwd, &["justfile", "Justfile", ".justfile"]) else { return vec![] };
    just_recipes_in(&text).into_iter().take(MAX_NAMES).map(|(n, d)| suggestion(n, d)).collect()
}

// ---- Cargo, workspaces ----------------------------------------------------------------------------

/// What `Cargo.toml` says that completion needs, read line by line (no TOML parser: the layout is regular).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub package: Option<String>,
    pub bins: Vec<String>,
    pub examples: Vec<String>,
    pub features: Vec<String>,
    pub members: Vec<String>,
    /// Crates under `[dependencies]`, `[dev-dependencies]` and `[build-dependencies]` (plain tables only), with which.
    pub dependencies: Vec<(String, &'static str)>,
}

const DEPENDENCY_TABLES: [(&str, &str); 3] = [("dependencies", "dependency"), ("dev-dependencies", "dev dependency"), ("build-dependencies", "build dependency")];

pub fn manifest_of(text: &str) -> Manifest {
    let mut m = Manifest::default();
    let mut section = String::new();
    let mut in_members = false;
    let unquote = |v: &str| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            in_members = false;
            // `[dependencies.serde]` names the crate in the header.
            for (table, what) in DEPENDENCY_TABLES {
                if let Some(name) = section.strip_prefix(table).and_then(|n| n.strip_prefix('.')) {
                    m.dependencies.push((unquote(name), what));
                }
            }
            continue;
        }
        if section == "workspace" && (line.starts_with("members") || in_members) {
            let start = if in_members { line } else { line.split_once('=').map_or("", |(_, v)| v) };
            in_members = !start.contains(']');
            m.members.extend(start.split('"').skip(1).step_by(2).map(str::to_string));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match section.as_str() {
            "package" if key == "name" => m.package = Some(unquote(value)),
            "bin" if key == "name" => m.bins.push(unquote(value)),
            "example" if key == "name" => m.examples.push(unquote(value)),
            "features" if plain(key) && key != "default" && !key.contains('.') => m.features.push(key.to_string()),
            // `serde.workspace = true` and `serde = "1"` both name `serde`.
            s => {
                if let Some((_, what)) = DEPENDENCY_TABLES.iter().find(|(table, _)| *table == s) {
                    m.dependencies.push((unquote(key.split('.').next().unwrap_or(key)), what));
                }
            }
        }
    }
    m
}

/// `*.rs` files and `*/main.rs` directories under `dir`: how Cargo discovers binaries and examples.
fn rust_targets(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else { return vec![] };
    let mut out = Vec::new();
    for e in read.flatten() {
        let Ok(name) = e.file_name().into_string() else { continue };
        let path = e.path();
        if path.is_dir() && path.join("main.rs").is_file() {
            out.push(name);
        } else if let Some(stem) = name.strip_suffix(".rs") {
            out.push(stem.to_string());
        }
    }
    out.retain(|n| plain(n));
    out.sort();
    out
}

/// The package the working directory belongs to: its folder and the manifest in it.
fn cargo_package(cwd: &Path) -> Option<(PathBuf, Manifest)> {
    cwd.ancestors().find_map(|d| {
        let m = manifest_of(&read_small(&d.join("Cargo.toml"))?);
        (m.package.is_some() || !m.members.is_empty()).then(|| (d.to_path_buf(), m))
    })
}

fn names(list: Vec<String>, what: &str) -> Vec<Suggestion> {
    let mut seen = HashSet::new();
    list.into_iter().filter(|n| plain(n) && seen.insert(n.clone())).take(MAX_NAMES).map(|n| suggestion(n, Some(what.into()))).collect()
}

fn cargo_bins(cwd: &Path) -> Vec<Suggestion> {
    let Some((dir, m)) = cargo_package(cwd) else { return vec![] };
    let mut all = m.bins;
    if dir.join("src/main.rs").is_file() {
        all.extend(m.package);
    }
    all.extend(rust_targets(&dir.join("src/bin")));
    names(all, "binary")
}

fn cargo_examples(cwd: &Path) -> Vec<Suggestion> {
    let Some((dir, m)) = cargo_package(cwd) else { return vec![] };
    let mut all = m.examples;
    all.extend(rust_targets(&dir.join("examples")));
    names(all, "example")
}

fn cargo_features(cwd: &Path) -> Vec<Suggestion> {
    cargo_package(cwd).map_or_else(Vec::new, |(_, m)| names(m.features, "feature"))
}

fn cargo_dependencies(cwd: &Path) -> Vec<Suggestion> {
    cargo_package(cwd).map_or_else(Vec::new, |(_, m)| described(m.dependencies, MAX_NAMES))
}

/// What `npm uninstall` / `pnpm remove` / `yarn remove` / `bun remove` can take out: the nearest `package.json`'s dependencies.
fn package_dependencies(cwd: &Path) -> Vec<Suggestion> {
    let Some(json) = cwd.ancestors().find_map(|d| read_small(&d.join("package.json"))).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return vec![] };
    let tables = [("dependencies", "dependency"), ("devDependencies", "dev dependency"), ("optionalDependencies", "optional dependency")];
    let all = tables.iter().flat_map(|(key, what)| json.get(key).and_then(|v| v.as_object()).into_iter().flat_map(move |o| o.keys().map(move |k| (k.clone(), *what))));
    described(all, MAX_NAMES)
}

/// Package names for `cargo -p` and `pnpm --filter`: the members of a Cargo workspace and the packages of an npm/pnpm one.
fn workspace_packages(cwd: &Path) -> Vec<Suggestion> {
    let mut out = Vec::new();
    for dir in cwd.ancestors() {
        let cargo = read_small(&dir.join("Cargo.toml")).map(|t| manifest_of(&t)).filter(|m| !m.members.is_empty());
        if let Some(m) = cargo {
            for pattern in &m.members {
                for member in expand_member(dir, pattern) {
                    if let Some(name) = read_small(&member.join("Cargo.toml")).and_then(|t| manifest_of(&t).package) {
                        out.push((name, "workspace crate"));
                    }
                }
            }
        }
        let mut patterns: Vec<String> = read_small(&dir.join("pnpm-workspace.yaml"))
            .map(|t| t.lines().filter_map(|l| l.trim().strip_prefix("- ")).map(|p| p.trim().trim_matches(|c| c == '"' || c == '\'').to_string()).collect())
            .unwrap_or_default();
        if let Some(v) = read_small(&dir.join("package.json")).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
            let list = v.get("workspaces").and_then(|w| w.as_array().or_else(|| w.get("packages")?.as_array()));
            patterns.extend(list.into_iter().flatten().filter_map(|p| p.as_str().map(str::to_string)));
        }
        for pattern in patterns.iter().filter(|p| !p.starts_with('!')) {
            for member in expand_member(dir, pattern) {
                let name = read_small(&member.join("package.json")).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v.get("name")?.as_str().map(str::to_string));
                if let Some(name) = name {
                    out.push((name, "workspace package"));
                }
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    described(out, MAX_NAMES)
}

/// A workspace member pattern: a folder, or `dir/*` for each folder in it. Deeper globs are not expanded.
fn expand_member(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    match pattern.strip_suffix("/*") {
        Some(parent) if !parent.contains('*') => {
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join(parent))
                .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).take(500).collect())
                .unwrap_or_default();
            dirs.sort();
            dirs
        }
        None if !pattern.contains('*') => vec![root.join(pattern)],
        _ => vec![],
    }
}

// ---- package.json, Makefile ---------------------------------------------------------------------

/// Keys of `scripts` in the nearest `package.json`, with the command each runs.
pub fn scripts_in(json: &str) -> Vec<(String, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    v.get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

fn package_scripts(cwd: &Path) -> Vec<Suggestion> {
    let Some(text) = cwd.ancestors().find_map(|d| read_small(&d.join("package.json"))) else { return vec![] };
    scripts_in(&text)
        .into_iter()
        .filter(|(k, _)| !k.starts_with('-') && !k.chars().any(|c| c.is_control() || c.is_whitespace()))
        .map(|(k, cmd)| suggestion(k, Some(cmd.chars().take(80).collect())))
        .take(MAX_NAMES)
        .collect()
}

/// Names in a TOML table and its `[table.name]` subtables, with the value of a `name = "..."` line.
fn toml_keys(text: &str, tables: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<&str> = None;
    let unquote = |v: &str| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|h| h.split(']').next()) {
            let header = header.trim();
            current = tables.iter().copied().find(|t| *t == header);
            // `[tool.pdm.scripts.serve]` names a script in its header.
            if let Some(name) = tables.iter().find_map(|t| header.strip_prefix(t)?.strip_prefix('.')) {
                out.push((unquote(name), "script".to_string()));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| current.is_some() && !line.starts_with('#')) else { continue };
        let key = unquote(key);
        // A plain command, or a table such as `{ cmd = "..." }`: show the command when it is a string.
        let value = value.trim();
        let shown = if value.starts_with('"') || value.starts_with('\'') { unquote(value) } else { "script".to_string() };
        if !key.contains('.') {
            out.push((key, shown));
        }
    }
    out
}

/// What `poetry run`, `uv run`, `pdm run` and `pipenv run` can take from the project: scripts and tasks of the
/// nearest `pyproject.toml`, and the `[scripts]` of a `Pipfile`.
fn python_scripts(cwd: &Path) -> Vec<Suggestion> {
    const TABLES: [&str; 7] = ["project.scripts", "tool.poetry.scripts", "tool.pdm.scripts", "tool.rye.scripts", "tool.poe.tasks", "tool.hatch.envs.default.scripts", "tool.uv.scripts"];
    let mut all = Vec::new();
    for dir in cwd.ancestors() {
        let pyproject = read_small(&dir.join("pyproject.toml"));
        let pipfile = read_small(&dir.join("Pipfile"));
        all.extend(pyproject.iter().flat_map(|t| toml_keys(t, &TABLES)));
        all.extend(pipfile.iter().flat_map(|t| toml_keys(t, &["scripts"])));
        if pyproject.is_some() || pipfile.is_some() {
            break;
        }
    }
    described(all.into_iter().map(|(n, d)| (n, d.chars().take(80).collect::<String>())), MAX_NAMES)
}

/// `tasks` of the nearest `deno.json` or `deno.jsonc` (whole-line `//` comments allowed), with the command each runs.
fn deno_tasks(cwd: &Path) -> Vec<Suggestion> {
    let Some(text) = nearest(cwd, &["deno.json", "deno.jsonc"]) else { return vec![] };
    let json: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { return vec![] };
    let tasks = v.get("tasks").and_then(|t| t.as_object()).into_iter().flatten().map(|(name, task)| {
        let command = task.as_str().or_else(|| task.get("description")?.as_str()).or_else(|| task.get("command")?.as_str()).unwrap_or("task");
        (name.clone(), command.chars().take(80).collect::<String>())
    });
    described(tasks, MAX_NAMES)
}

/// Programs installed in the nearest `node_modules/.bin`.
fn node_bins(cwd: &Path) -> Vec<Suggestion> {
    let Some(bin) = cwd.ancestors().map(|d| d.join("node_modules/.bin")).find(|b| b.is_dir()) else { return vec![] };
    let mut found: Vec<String> = std::fs::read_dir(&bin).map(|r| r.flatten().filter_map(|e| e.file_name().into_string().ok()).take(MAX_NAMES).collect()).unwrap_or_default();
    found.sort();
    names(found, "node_modules/.bin")
}

/// Rule targets of a Makefile: `name:` at the start of a line (not `:=`, not patterns, not `.PHONY`-style specials).
pub fn make_targets_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t', '#', '.']) {
            continue;
        }
        let Some((targets, rest)) = line.split_once(':') else { continue };
        if rest.starts_with('=') || targets.contains(['=', '%', '$', '(']) {
            continue;
        }
        out.extend(
            targets
                .split_whitespace()
                .filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+')))
                .map(str::to_string),
        );
    }
    let mut seen = HashSet::new();
    out.retain(|t| seen.insert(t.clone()));
    out
}

fn make_targets(cwd: &Path) -> Vec<Suggestion> {
    let Some(text) = ["GNUmakefile", "makefile", "Makefile"].iter().find_map(|f| read_small(&cwd.join(f))) else { return vec![] };
    make_targets_in(&text).into_iter().take(MAX_NAMES).map(|t| suggestion(t, Some("make target".into()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let git = d.path().join(".git");
        std::fs::create_dir_all(git.join("refs/heads/feature")).unwrap();
        std::fs::create_dir_all(git.join("refs/remotes/origin")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(git.join("refs/heads/main"), "a\n").unwrap();
        std::fs::write(git.join("refs/heads/feature/login"), "a\n").unwrap();
        std::fs::write(git.join("refs/remotes/origin/main"), "a\n").unwrap();
        std::fs::write(git.join("refs/remotes/origin/HEAD"), "ref: refs/remotes/origin/main\n").unwrap();
        std::fs::write(git.join("refs/remotes/origin/hotfix"), "a\n").unwrap();
        std::fs::write(git.join("packed-refs"), "# pack-refs\nabc refs/heads/release\nabc refs/remotes/origin/staging\n^abc\n").unwrap();
        std::fs::write(git.join("config"), "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@host:me/x.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[remote \"fork\"]\n\turl = https://f/x\n[branch \"main\"]\n\tremote = origin\n").unwrap();
        d
    }

    #[test]
    fn branches_are_local_first_with_the_current_one_leading_then_remote_only_ones() {
        let d = repo();
        let sub = d.path().join("src/deep");
        std::fs::create_dir_all(&sub).unwrap();
        let b = git_branches(&sub); // found from a subdirectory
        let names: Vec<_> = b.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["main", "feature/login", "release", "hotfix", "staging"]);
        assert_eq!(b[0].description.as_deref(), Some("current branch"));
        assert_eq!(b[3].description.as_deref(), Some("remote branch (origin)"));
    }

    #[test]
    fn remotes_come_with_their_urls() {
        let d = repo();
        let r = git_remotes(d.path());
        assert_eq!(r.iter().map(|s| (s.name.as_str(), s.description.as_deref())).collect::<Vec<_>>(), [("origin", Some("git@host:me/x.git")), ("fork", Some("https://f/x"))]);
    }

    #[test]
    fn a_linked_worktree_reads_the_main_repos_refs() {
        let main = repo();
        let wt_git = main.path().join(".git/worktrees/wt");
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feature/login\n").unwrap();
        std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        let wt = tempfile::tempdir().unwrap();
        std::fs::write(wt.path().join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        let b = git_branches(wt.path());
        assert_eq!(b[0].name, "feature/login");
        assert_eq!(b[0].description.as_deref(), Some("current branch"));
        assert!(b.iter().any(|s| s.name == "main"));
    }

    #[test]
    fn outside_a_repository_there_is_nothing() {
        let d = tempfile::tempdir().unwrap();
        assert!(git_branches(d.path()).is_empty() && git_remotes(d.path()).is_empty());
    }

    #[test]
    fn scripts_come_from_the_nearest_package_json() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("package.json"), r#"{"name":"x","scripts":{"dev":"vite","build":"tsc && vite build","bad name":"x"}}"#).unwrap();
        let sub = d.path().join("packages/a");
        std::fs::create_dir_all(&sub).unwrap();
        let s = package_scripts(&sub);
        assert_eq!(s.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["build", "dev"]);
        assert_eq!(s[1].description.as_deref(), Some("vite"));
    }

    #[test]
    fn git_aliases_come_from_the_repository_and_your_config_without_shell_ones() {
        assert_eq!(
            aliases_in("[core]\n\tbare = false\n[alias]\n\tco = checkout\n\tsw = \"switch -c\"\n\tup = !git pull\n; note = x\n[Alias]\n\tst = status -sb\n[user]\n\tname = me\n"),
            [("co", "checkout"), ("sw", "switch -c"), ("st", "status -sb")].map(|(a, b)| (a.to_string(), b.to_string()))
        );
        let d = repo();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".gitconfig"), "[alias]\n  co = checkout\n  lg = log --graph\n").unwrap();
        std::fs::write(d.path().join(".git/config"), "[alias]\n  co = commit\n").unwrap();
        let found: Vec<_> = git_aliases(d.path(), home.path()).into_iter().map(|s| (s.name, s.description.unwrap())).collect();
        assert_eq!(found, [("co", "commit"), ("lg", "log --graph")].map(|(a, b)| (a.to_string(), b.to_string())), "the repository's own alias wins");
    }

    #[test]
    fn python_scripts_deno_tasks_and_node_bins_come_from_the_nearest_project() {
        let d = tempfile::tempdir().unwrap();
        let sub = d.path().join("src/app");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(d.path().join("pyproject.toml"), "[project]\nname = \"x\"\n[project.scripts]\ncli = \"x.main:run\"\n[tool.pdm.scripts]\nlint = \"ruff check .\"\ntest = { cmd = \"pytest\" }\n[tool.pdm.scripts.serve]\ncmd = \"uvicorn x:app\"\n[tool.ruff]\nline-length = 100\n").unwrap();
        let found: Vec<_> = python_scripts(&sub).into_iter().map(|s| (s.name, s.description.unwrap())).collect();
        assert_eq!(found, [("cli", "x.main:run"), ("lint", "ruff check ."), ("test", "script"), ("serve", "script")].map(|(a, b)| (a.to_string(), b.to_string())));
        std::fs::write(sub.join("Pipfile"), "[packages]\nrequests = \"*\"\n[scripts]\nstart = \"python app.py\"\n").unwrap();
        assert_eq!(python_scripts(&sub).into_iter().map(|s| s.name).collect::<Vec<_>>(), ["start"], "the nearest project only");

        std::fs::write(d.path().join("deno.jsonc"), "{\n  // tasks\n  \"tasks\": { \"dev\": \"deno run -A main.ts\", \"check\": { \"command\": \"deno check\", \"description\": \"Type-check\" } }\n}\n").unwrap();
        let tasks: Vec<_> = deno_tasks(&sub).into_iter().map(|s| (s.name, s.description.unwrap())).collect();
        assert_eq!(tasks, [("check", "Type-check"), ("dev", "deno run -A main.ts")].map(|(a, b)| (a.to_string(), b.to_string())));

        std::fs::create_dir_all(d.path().join("node_modules/.bin")).unwrap();
        for b in ["vite", "eslint"] {
            std::fs::write(d.path().join("node_modules/.bin").join(b), "").unwrap();
        }
        assert_eq!(node_bins(&sub).into_iter().map(|s| s.name).collect::<Vec<_>>(), ["eslint", "vite"]);
    }

    #[test]
    fn make_targets_skip_assignments_patterns_and_specials() {
        let t = "CC := gcc\nVERSION=1\n.PHONY: all test\nall: build test\nbuild test: deps\n\t@echo hi\n%.o: %.c\n\t$(CC)\n$(OUT): x\nclean:\nall:\n\n# note: here\n";
        assert_eq!(make_targets_in(t), ["all", "build", "test", "clean"]);
    }

    #[test]
    fn tags_list_newest_version_first_and_stashes_newest_first() {
        let d = repo();
        let git = d.path().join(".git");
        std::fs::create_dir_all(git.join("refs/tags")).unwrap();
        std::fs::write(git.join("refs/tags/v1.9.0"), "a\n").unwrap();
        std::fs::write(git.join("refs/tags/v1.10.0"), "a\n").unwrap();
        let mut packed = std::fs::read_to_string(git.join("packed-refs")).unwrap();
        packed.push_str("abc refs/tags/v0.2.0\n^def\n");
        std::fs::write(git.join("packed-refs"), packed).unwrap();
        let tags = git_tags(d.path());
        assert_eq!(tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["v1.10.0", "v1.9.0", "v0.2.0"]);

        std::fs::create_dir_all(git.join("logs/refs")).unwrap();
        std::fs::write(git.join("logs/refs/stash"), "0 a A <a@b> 1 +0000\tWIP on main: one\n0 b A <a@b> 2 +0000\tOn main: two\n").unwrap();
        let stashes = git_stashes(d.path());
        assert_eq!((stashes[0].name.as_str(), stashes[0].description.as_deref(), stashes[0].insert_value.as_deref()), ("stash@{0}", Some("On main: two"), Some("stash@{0} ")));
        assert_eq!(stashes[1].name, "stash@{1}");
    }

    #[test]
    fn compose_services_and_taskfile_tasks_are_the_children_of_their_mapping() {
        let compose = "name: app\nservices:\n  web:\n    image: \"nginx:1\"\n    ports:\n      - 80:80\n  # db is next\n  db:\n    build: .\nvolumes:\n  data: {}\n";
        assert_eq!(compose_services_in(compose), [("web".to_string(), Some("nginx:1".to_string())), ("db".to_string(), None)]);
        let task = "version: '3'\ntasks:\n  build:\n    desc: Build it\n    cmds:\n      - go build\n  test:\n    cmds: [go test]\nvars:\n  X: y\n";
        assert_eq!(taskfile_tasks_in(task), [("build".to_string(), Some("Build it".to_string())), ("test".to_string(), None)]);
    }

    #[test]
    fn just_recipes_keep_comments_and_skip_private_ones_and_settings() {
        let text = "set shell := [\"bash\", \"-c\"]\nexport FOO := \"1\"\nversion := \"1\"\n\n# Build everything\nbuild target=\"all\":\n    cargo build\n\n_helper:\n    echo\n\n@test *args:\n    cargo test\nalias t := test\n";
        assert_eq!(just_recipes_in(text), [("build".to_string(), Some("Build everything".to_string())), ("test".to_string(), None)]);
    }

    #[test]
    fn cargo_targets_features_and_workspace_members_come_from_manifests() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Cargo.toml"), "[workspace]\nmembers = [\n  \"crates/*\",\n  \"tools/cli\",\n]\n").unwrap();
        for (dir, name) in [("crates/core", "my-core"), ("crates/web", "my-web"), ("tools/cli", "my-cli")] {
            std::fs::create_dir_all(d.path().join(dir).join("src")).unwrap();
            std::fs::write(d.path().join(dir).join("Cargo.toml"), format!("[package]\nname = \"{name}\"\n")).unwrap();
        }
        let app = d.path().join("crates/web");
        std::fs::write(app.join("Cargo.toml"), "[package]\nname = \"my-web\"\n[features]\ndefault = [\"tls\"]\ntls = []\nmetrics = [\"dep:prom\"]\n[[bin]]\nname = \"web-admin\"\npath = \"src/admin.rs\"\n").unwrap();
        std::fs::write(app.join("src/main.rs"), "").unwrap();
        std::fs::create_dir_all(app.join("src/bin/tool")).unwrap();
        std::fs::write(app.join("src/bin/tool/main.rs"), "").unwrap();
        std::fs::write(app.join("src/bin/seed.rs"), "").unwrap();
        std::fs::create_dir_all(app.join("examples")).unwrap();
        std::fs::write(app.join("examples/hello.rs"), "").unwrap();
        let names = |v: Vec<Suggestion>| v.into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names(cargo_bins(&app.join("src"))), ["web-admin", "my-web", "seed", "tool"]);
        assert_eq!(names(cargo_examples(&app)), ["hello"]);
        assert_eq!(names(cargo_features(&app)), ["tls", "metrics"]);
        assert_eq!(names(workspace_packages(&app)), ["my-core", "my-web", "my-cli"]);
    }

    #[test]
    fn dependencies_come_from_the_nearest_manifests_and_fail_open() {
        let d = tempfile::tempdir().unwrap();
        let sub = d.path().join("packages/a");
        std::fs::create_dir_all(&sub).unwrap();
        let names = |v: Vec<Suggestion>| v.into_iter().map(|s| (s.name, s.description.unwrap())).collect::<Vec<_>>();
        assert!(package_dependencies(&sub).is_empty() && cargo_dependencies(&sub).is_empty(), "no manifest: nothing");

        std::fs::write(d.path().join("package.json"), r#"{"dependencies":{"react":"^18","@scope/ui":"1"},"devDependencies":{"vite":"5","bad name":"1","-rf":"1"},"optionalDependencies":{"fsevents":"2"}}"#).unwrap();
        assert_eq!(
            names(package_dependencies(&sub)),
            [("@scope/ui", "dependency"), ("react", "dependency"), ("vite", "dev dependency"), ("fsevents", "optional dependency")]
                .map(|(n, w)| (n.to_string(), w.to_string()))
        );
        std::fs::write(d.path().join("package.json"), "{ not json").unwrap();
        assert!(package_dependencies(&sub).is_empty(), "a broken manifest fails open");

        let manifest = "[package]\nname = \"app\"\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nanyhow.workspace = true # shared\nlog = \"0.4\"\n[dev-dependencies]\ntempfile = \"3\"\n[build-dependencies.cc]\nversion = \"1\"\n[features]\ntls = []\n";
        std::fs::write(d.path().join("Cargo.toml"), manifest).unwrap();
        assert_eq!(
            names(cargo_dependencies(&sub)),
            [("serde", "dependency"), ("anyhow", "dependency"), ("log", "dependency"), ("tempfile", "dev dependency"), ("cc", "build dependency")]
                .map(|(n, w)| (n.to_string(), w.to_string()))
        );
    }

    #[test]
    fn npm_and_pnpm_workspaces_list_package_names() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("pnpm-workspace.yaml"), "packages:\n  - 'apps/*'\n  - \"!apps/skip\"\n").unwrap();
        for (dir, name) in [("apps/web", "@me/web"), ("apps/api", "@me/api")] {
            std::fs::create_dir_all(d.path().join(dir)).unwrap();
            std::fs::write(d.path().join(dir).join("package.json"), format!("{{\"name\":\"{name}\"}}")).unwrap();
        }
        let mut found: Vec<_> = workspace_packages(&d.path().join("apps/web")).into_iter().map(|s| s.name).collect();
        found.sort();
        assert_eq!(found, ["@me/api", "@me/web"]);
    }
}
