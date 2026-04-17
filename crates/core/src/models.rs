//! Data models and path constants.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("{0}")]
    Msg(String),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Workspace,
}

impl Scope {
    pub fn from_str(s: &str) -> Self {
        if s == "workspace" {
            Self::Workspace
        } else {
            Self::Global
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
        }
    }
}

// ── paths ────────────────────────────────────────────────────────────────

pub fn kiro_home() -> PathBuf {
    dirs::home_dir().expect("no home dir").join(".kiro")
}

pub fn kiro_workspace() -> PathBuf {
    PathBuf::from(".kiro")
}

pub fn kiro_root(scope: Scope) -> PathBuf {
    match scope {
        Scope::Global => kiro_home(),
        Scope::Workspace => kiro_workspace(),
    }
}

pub fn converter_home() -> PathBuf {
    kiro_home().join("cc-plugins")
}

pub fn cache_dir() -> PathBuf {
    converter_home().join("cache")
}

pub fn config_file() -> PathBuf {
    converter_home().join("config.json")
}

pub fn registry_file() -> PathBuf {
    converter_home().join("registry.json")
}

pub fn ensure_dirs() -> Result<()> {
    std::fs::create_dir_all(cache_dir())?;
    Ok(())
}

// ── models ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub cloned_at: String,
    #[serde(default)]
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRecord {
    #[serde(rename = "type")]
    pub component_type: String,
    pub name: String,
    pub source_rel: String,
    pub target_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub plugin_name: String,
    pub source_name: String,
    #[serde(default)]
    pub components: Vec<ComponentRecord>,
    #[serde(default)]
    pub installed_at: String,
    #[serde(default)]
    pub source_commit: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "global".to_string()
}

// ── JSON helpers ─────────────────────────────────────────────────────────

pub fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &std::path::Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn save_json<T: Serialize>(path: &std::path::Path, data: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(data)?;
    std::fs::write(path, format!("{text}\n"))?;
    Ok(())
}
