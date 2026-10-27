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
    pub template: Option<Template>,
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
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
