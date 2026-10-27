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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opt {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub takes_value: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Filepaths,
    Folders,
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
