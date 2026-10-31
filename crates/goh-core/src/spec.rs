//! Completion spec v1: declarative, non-executable (docs/schemas/completion.schema.json).

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub subcommands: Vec<Command>,
    #[serde(default)]
    pub options: Vec<Opt>,
    #[serde(default)]
    pub args: Vec<Arg>,
    /// The spec applies only where one of these files is in the working directory or above it (`task` is
    /// go-task's only next to a Taskfile; elsewhere it may be Taskwarrior).
    #[serde(default)]
    pub requires_files: Vec<String>,
    /// Where the user's own names for subcommands come from (`git co` from `[alias] co = checkout` in
    /// git's config): each value's description is what the name stands for.
    #[serde(default)]
    pub subcommand_aliases: Option<Provider>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opt {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub takes_value: bool,
    /// Offered again after it was given (`-v -v`). Options that take a value are always offered again.
    #[serde(default)]
    pub is_repeatable: bool,
    #[serde(default)]
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arg {
    #[serde(default)]
    pub is_variadic: bool,
    #[serde(default)]
    pub provider: Option<Provider>,
    /// More providers for the same slot (`git diff` takes a branch or a changed file).
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub is_command: bool,
    /// The rest of the line is a command this one runs on this machine (`sudo`, `time`, `npx`): its first
    /// word completes as a command and the words after it by that command's spec.
    #[serde(default)]
    pub runs_command: bool,
    #[serde(default)]
    pub template: Option<Template>,
    /// With `filepaths`: only files ending in one of these (without the dot, e.g. `py`, `tar.gz`); directories stay.
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
}

/// Fixed, read-only providers implemented by the daemon (Docker queries and frequently used directories). Specs cannot supply executable scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    DockerContainers,
    DockerAllContainers,
    DockerImages,
    FrecentDirs,
    SshHosts,
    SshRemotes,
    KubeContexts,
    AwsProfiles,
    GitBranches,
    GitRemotes,
    GitTags,
    GitStashes,
    /// Files with changes not yet staged, plus untracked ones (`git add`, `restore`, `diff`).
    GitChangedFiles,
    /// Files with staged changes (`git restore --staged`, `reset`, `diff --cached`).
    GitStagedFiles,
    PackageScripts,
    MakeTargets,
    JustRecipes,
    TaskfileTasks,
    ComposeServices,
    CargoBins,
    CargoExamples,
    CargoFeatures,
    WorkspacePackages,
    /// What `npm uninstall`, `pnpm remove`, `yarn remove` and `bun remove` can take out: the nearest `package.json`'s dependencies.
    PackageDependencies,
    /// The crates in the nearest `Cargo.toml`'s dependency tables (`cargo remove`).
    CargoDependencies,
    /// Crates installed with `cargo install` (`cargo uninstall`).
    CargoInstalled,
    /// Python packages installed for `pip`: the project's virtualenv if it has one, else the user and system site-packages.
    PipPackages,
    /// Tools installed with `uv tool install`.
    UvTools,
    /// Installed Homebrew formulae and casks (`brew uninstall`, `upgrade`, `list`).
    BrewInstalled,
    /// Every Homebrew formula and cask Homebrew knows of, from its own local cache of the API (`brew install`, `info`).
    BrewAvailable,
    /// `[alias]` entries of git's config (yours and the repository's), described by what they stand for.
    GitAliases,
    /// Recent commits of the current branch, newest first, with their subject.
    GitCommits,
    /// Pods of the current kubectl context and namespace.
    KubePods,
    /// Namespaces of the current kubectl context.
    KubeNamespaces,
    /// Scripts of the nearest `pyproject.toml` (`[project.scripts]`, Poetry, PDM, Rye, Hatch, Poe) and `Pipfile`.
    PythonScripts,
    /// Tasks of the nearest `deno.json` / `deno.jsonc`.
    DenoTasks,
    /// Programs in the nearest `node_modules/.bin` (`npx`, `pnpm exec`, `yarn run`).
    NodeBins,
    /// Docker networks (`docker network rm`, `connect`, `inspect`).
    DockerNetworks,
    /// Docker volumes (`docker volume rm`, `inspect`).
    DockerVolumes,
    /// tmux sessions of the default server (`tmux attach-session -t`, `kill-session -t`).
    TmuxSessions,
    /// tmux windows as `session:index` (`tmux select-window -t`).
    TmuxWindows,
    /// systemd units, loaded or not (`systemctl restart`, `status`, `enable`).
    SystemdUnits,
    /// Running processes by pid, described by their name (`kill`).
    Processes,
    /// Names of running processes (`pkill`, `killall`).
    ProcessNames,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Filepaths,
    Folders,
    /// Names of commands: those with a spec and what the shell can run (`man`, `which`).
    Commands,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub insert_value: Option<String>,
}

impl Command {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
