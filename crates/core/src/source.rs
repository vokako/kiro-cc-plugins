//! Git source management and marketplace parsing.

use crate::models::{cache_dir, config_file, load_json, save_json, Error, Result, Source};
use chrono::Utc;
use git2::build::RepoBuilder;
use git2::{FetchOptions, Oid, Repository};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ConfigFile {
    #[serde(default)]
    sources: Vec<Source>,
}

fn cache_dir_for(name: &str) -> PathBuf {
    cache_dir().join(name.replace('/', "--"))
}

fn external_cache() -> PathBuf {
    cache_dir().join("_external")
}

fn git_head(repo_dir: &Path) -> Result<String> {
    let repo = Repository::open(repo_dir)?;
    let head = repo.head()?;
    let oid = head.target().ok_or_else(|| Error::msg("HEAD has no target"))?;
    Ok(oid.to_string())
}

fn clone_repo(url: &str, dest: &Path, ref_or_sha: Option<&str>) -> Result<()> {
    let mut fo = FetchOptions::new();
    fo.depth(1);
    let mut builder = RepoBuilder::new();
    builder.fetch_options(fo);
    if let Some(r) = ref_or_sha {
        // Only useful for branch refs; SHA handled separately below.
        builder.branch(r);
    }
    let repo = builder.clone(url, dest)?;
    // If caller requested a specific SHA, fetch and checkout
    drop(repo);
    Ok(())
}

fn pull_ff(repo_dir: &Path) -> Result<()> {
    // libgit2's shallow fetch is unreliable (ACK parsing errors, missing objects).
    // Since these are read-only caches, just re-clone for a clean state.
    let url = {
        let repo = Repository::open(repo_dir)?;
        let remote = repo.find_remote("origin")?;
        remote.url().unwrap_or("").to_string()
    };
    if url.is_empty() {
        return Err(Error::msg("cannot re-clone: no origin URL"));
    }
    std::fs::remove_dir_all(repo_dir)?;
    clone_repo(&url, repo_dir, None)?;
    Ok(())
}

fn derive_source_name(url: &str) -> String {
    let s = url.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        parts.last().copied().unwrap_or(s).to_string()
    }
}

// ── public API ───────────────────────────────────────────────────────────

pub fn list_sources() -> Vec<Source> {
    let cfg: ConfigFile = load_json(&config_file()).unwrap_or_default();
    cfg.sources
}

pub fn add_source(url: &str, name: Option<&str>) -> Result<Source> {
    let name = name.map(|s| s.to_string()).unwrap_or_else(|| derive_source_name(url));
    let repo_dir = cache_dir_for(&name);
    std::fs::create_dir_all(cache_dir())?;
    if repo_dir.exists() {
        pull_ff(&repo_dir)?;
    } else {
        clone_repo(url, &repo_dir, None)?;
    }
    let commit = git_head(&repo_dir)?;
    let src = Source {
        name: name.clone(),
        url: url.to_string(),
        cloned_at: Utc::now().to_rfc3339(),
        commit,
    };

    let mut cfg: ConfigFile = load_json(&config_file()).unwrap_or_default();
    cfg.sources.retain(|s| s.name != name);
    cfg.sources.push(src.clone());
    save_json(&config_file(), &cfg)?;
    Ok(src)
}

pub fn update_source(name: &str) -> Result<String> {
    let repo_dir = cache_dir_for(name);
    if !repo_dir.exists() {
        return Err(Error::msg(format!("Source '{name}' not found in cache")));
    }
    pull_ff(&repo_dir)?;
    let commit = git_head(&repo_dir)?;

    let mut cfg: ConfigFile = load_json(&config_file()).unwrap_or_default();
    for s in &mut cfg.sources {
        if s.name == name {
            s.commit = commit.clone();
        }
    }
    save_json(&config_file(), &cfg)?;
    Ok(commit)
}

pub fn remove_source(name: &str) -> Result<()> {
    let repo_dir = cache_dir_for(name);
    if repo_dir.exists() {
        std::fs::remove_dir_all(&repo_dir)?;
    }
    let mut cfg: ConfigFile = load_json(&config_file()).unwrap_or_default();
    cfg.sources.retain(|s| s.name != name);
    save_json(&config_file(), &cfg)?;
    Ok(())
}

pub fn get_source_dir(name: &str) -> Result<PathBuf> {
    let d = cache_dir_for(name);
    if !d.exists() {
        return Err(Error::msg(format!("Source '{name}' not cached")));
    }
    Ok(d)
}

// ── external plugin fetch ────────────────────────────────────────────────

fn normalize_git_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("git@") {
        url.to_string()
    } else {
        format!("https://github.com/{url}.git")
    }
}

fn clone_or_pull(dest: &Path, url: &str, reference: Option<&str>, sha: Option<&str>) -> Result<Option<PathBuf>> {
    let res: Result<()> = (|| {
        if dest.exists() {
            pull_ff(dest)?;
            if let Some(sha_str) = sha {
                let repo = Repository::open(dest)?;
                let oid = Oid::from_str(sha_str)?;
                let obj = repo.find_object(oid, None)?;
                repo.checkout_tree(&obj, None)?;
                repo.set_head_detached(oid)?;
            }
        } else {
            clone_repo(url, dest, reference.filter(|_| sha.is_none()))?;
            if let Some(sha_str) = sha {
                let repo = Repository::open(dest)?;
                let oid = Oid::from_str(sha_str)?;
                let obj = repo.find_object(oid, None)?;
                repo.checkout_tree(&obj, None)?;
                repo.set_head_detached(oid)?;
            }
        }
        Ok(())
    })();
    match res {
        Ok(()) => Ok(Some(dest.to_path_buf())),
        Err(_) => Ok(None),
    }
}

/// Fetch an external plugin referenced by a marketplace entry's `source` object.
pub fn fetch_external_plugin(plugin_name: &str, source_spec: &Value) -> Result<Option<PathBuf>> {
    std::fs::create_dir_all(external_cache())?;
    let dest = external_cache().join(plugin_name);

    let src_type = source_spec.get("source").and_then(|v| v.as_str()).unwrap_or("");

    let get_str = |k: &str| -> Option<String> {
        source_spec.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
    };

    match src_type {
        "url" => {
            let url = normalize_git_url(&get_str("url").unwrap_or_default());
            clone_or_pull(&dest, &url, get_str("ref").as_deref(), get_str("sha").as_deref())
        }
        "github" => {
            let repo = get_str("repo").unwrap_or_default();
            let url = format!("https://github.com/{repo}.git");
            clone_or_pull(&dest, &url, get_str("ref").as_deref(), get_str("sha").as_deref())
        }
        "git-subdir" => {
            let url = normalize_git_url(&get_str("url").unwrap_or_default());
            let subpath = get_str("path").unwrap_or_default();
            let repo_dir = clone_or_pull(&dest, &url, get_str("ref").as_deref(), get_str("sha").as_deref())?;
            if let Some(r) = repo_dir {
                if !subpath.is_empty() {
                    let full = r.join(&subpath);
                    return Ok(if full.is_dir() { Some(full) } else { None });
                }
                return Ok(Some(r));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn resolve_external_cached(plugin_name: &str, source_spec: &Value) -> Option<PathBuf> {
    let dest = external_cache().join(plugin_name);
    if !dest.is_dir() {
        return None;
    }
    let src_type = source_spec.get("source").and_then(|v| v.as_str()).unwrap_or("");
    if src_type == "git-subdir" {
        let subpath = source_spec.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if !subpath.is_empty() {
            let full = dest.join(subpath);
            return if full.is_dir() { Some(full) } else { None };
        }
    }
    Some(dest)
}

// ── marketplace parsing ──────────────────────────────────────────────────

/// Parsed marketplace entry. Mirrors the Python dict shape (keys prefixed with `_`
/// are populated internally and not present in the on-disk JSON).
#[derive(Debug, Clone)]
pub struct MarketplacePlugin {
    pub name: String,
    pub description: String,
    pub category: String,
    pub source: Value,
    pub local_path: Option<PathBuf>,
    pub skill_filter: Option<Vec<String>>,
    pub has_lsp_servers: bool,
}

pub fn parse_marketplace(source_name: &str) -> Result<Vec<MarketplacePlugin>> {
    let repo_dir = get_source_dir(source_name)?;
    let mp_file = repo_dir.join(".claude-plugin").join("marketplace.json");

    if !mp_file.exists() {
        // Treat whole repo as a single plugin
        let pj = repo_dir.join(".claude-plugin").join("plugin.json");
        let (name, desc) = if pj.exists() {
            let d: Value = serde_json::from_str(&std::fs::read_to_string(&pj)?)?;
            (
                d.get("name").and_then(|v| v.as_str()).unwrap_or(source_name).to_string(),
                d.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            )
        } else {
            (source_name.to_string(), String::new())
        };
        return Ok(vec![MarketplacePlugin {
            name,
            description: desc,
            category: String::new(),
            source: Value::String(".".into()),
            local_path: Some(repo_dir),
            skill_filter: None,
            has_lsp_servers: false,
        }]);
    }

    let mp: Value = serde_json::from_str(&std::fs::read_to_string(&mp_file)?)?;
    let mut plugins = Vec::new();
    let empty = Vec::new();
    let entries = mp.get("plugins").and_then(|v| v.as_array()).unwrap_or(&empty);
    for entry in entries {
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let description = entry.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let category = entry.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let source_val = entry.get("source").cloned().unwrap_or(Value::Null);
        let skill_filter: Option<Vec<String>> = entry
            .get("skills")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());

        let local_path = match &source_val {
            Value::String(s) if s.starts_with('.') => {
                Some(repo_dir.join(s.trim_start_matches("./").trim_start_matches('.')))
            }
            Value::Object(_) => resolve_external_cached(&name, &source_val),
            _ => None,
        };

        let has_lsp_servers = entry.get("lspServers").is_some();

        plugins.push(MarketplacePlugin {
            name,
            description,
            category,
            source: source_val,
            local_path,
            skill_filter,
            has_lsp_servers,
        });
    }
    Ok(plugins)
}

/// Convert a MarketplacePlugin to a serde_json::Value dict (for API/registry compatibility).
pub fn plugin_to_json(p: &MarketplacePlugin) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(p.name.clone()));
    m.insert("description".into(), Value::String(p.description.clone()));
    m.insert("category".into(), Value::String(p.category.clone()));
    m.insert("source".into(), p.source.clone());
    if let Some(lp) = &p.local_path {
        m.insert("_local_path".into(), Value::String(lp.to_string_lossy().into()));
    } else {
        m.insert("_local_path".into(), Value::Null);
    }
    if let Some(sf) = &p.skill_filter {
        m.insert("_skill_filter".into(), Value::Array(sf.iter().map(|s| Value::String(s.clone())).collect()));
    }
    Value::Object(m)
}
