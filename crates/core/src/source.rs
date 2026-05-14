//! Git source management and marketplace parsing.

use crate::models::{cache_dir, config_file, load_json, save_json, Error, Result, Source};
use chrono::Utc;
use git2::build::RepoBuilder;
use git2::{Cred, CredentialType, FetchOptions, Oid, RemoteCallbacks, Repository};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Build a `RemoteCallbacks` whose credential handler tries common auth sources.
///
/// Resolution order:
///   1. `GH_TOKEN` / `GITHUB_TOKEN` env var (HTTPS, basic auth as `x-access-token`)
///   2. Git credential helper from system git config (osxkeychain, wincred, manager-core, …)
///   3. SSH agent (for `git@host:...` URLs)
///   4. Default SSH key files in `~/.ssh/` (id_ed25519, id_rsa, id_ecdsa)
///   5. libgit2 default (last resort)
fn make_remote_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(|url, username_from_url, allowed_types| {
        // SSH branch
        if allowed_types.contains(CredentialType::SSH_KEY) {
            let user = username_from_url.unwrap_or("git");
            // 1. ssh-agent
            if let Ok(cred) = Cred::ssh_key_from_agent(user) {
                return Ok(cred);
            }
            // 2. ~/.ssh/<key>
            if let Some(home) = dirs::home_dir() {
                for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
                    let key_path = home.join(".ssh").join(key_name);
                    if key_path.exists() {
                        if let Ok(cred) = Cred::ssh_key(user, None, &key_path, None) {
                            return Ok(cred);
                        }
                    }
                }
            }
        }

        // HTTPS userpass branch
        if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT) {
            // 1. GH_TOKEN / GITHUB_TOKEN
            if let Ok(token) = std::env::var("GH_TOKEN").or_else(|_| std::env::var("GITHUB_TOKEN")) {
                if !token.is_empty() {
                    return Cred::userpass_plaintext("x-access-token", &token);
                }
            }
            // 2. git credential helper (osxkeychain / wincred / cache / manager-core)
            if let Ok(config) = git2::Config::open_default() {
                if let Ok(cred) = Cred::credential_helper(&config, url, username_from_url) {
                    return Ok(cred);
                }
            }
        }

        // Username-only (some servers send this first to negotiate)
        if allowed_types.contains(CredentialType::USERNAME) {
            if let Some(user) = username_from_url {
                if let Ok(cred) = Cred::username(user) {
                    return Ok(cred);
                }
            }
        }

        // libgit2 default (last resort, e.g. SSH config defaults)
        if allowed_types.contains(CredentialType::DEFAULT) {
            if let Ok(cred) = Cred::default() {
                return Ok(cred);
            }
        }

        Err(git2::Error::from_str(
            "authentication required: set GH_TOKEN/GITHUB_TOKEN, configure git credential helper, or add an SSH key",
        ))
    });
    cb
}

/// FetchOptions with auth callbacks and a shallow depth.
fn make_fetch_options<'a>() -> FetchOptions<'a> {
    let mut fo = FetchOptions::new();
    fo.depth(1);
    fo.remote_callbacks(make_remote_callbacks());
    fo
}

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

/// Fetch the remote HEAD commit SHA without cloning/fetching objects.
/// Uses `git ls-remote`-equivalent via libgit2 remote connect.
pub fn remote_head(url: &str) -> Result<String> {
    // libgit2 here was built without libssh2 — skip it for SSH URLs.
    if is_ssh_url(url) {
        return shell_git_ls_remote(url);
    }
    match remote_head_libgit2(url) {
        Ok(sha) => Ok(sha),
        Err(libgit2_err) => match shell_git_ls_remote(url) {
            Ok(sha) => Ok(sha),
            Err(shell_err) => Err(Error::msg(format!(
                "ls-remote failed: {libgit2_err}\nfallback: {shell_err}"
            ))),
        },
    }
}

fn remote_head_libgit2(url: &str) -> Result<String> {
    let mut remote = git2::Remote::create_detached(url)?;
    remote.connect_auth(git2::Direction::Fetch, Some(make_remote_callbacks()), None)?;
    let list = remote.list()?;
    // Look for HEAD ref first, fall back to refs/heads/main or refs/heads/master
    let mut head_sha: Option<String> = None;
    let mut main_sha: Option<String> = None;
    let mut master_sha: Option<String> = None;
    for r in list.iter() {
        match r.name() {
            "HEAD" => head_sha = Some(r.oid().to_string()),
            "refs/heads/main" => main_sha = Some(r.oid().to_string()),
            "refs/heads/master" => master_sha = Some(r.oid().to_string()),
            _ => {}
        }
    }
    remote.disconnect()?;
    head_sha.or(main_sha).or(master_sha).ok_or_else(|| Error::msg("no HEAD on remote"))
}

/// Fall back to system `git ls-remote` — uses the user's full git environment
/// (credential helpers, custom SSH config, FIDO2 keys, enterprise auth, …).
fn shell_git_ls_remote(url: &str) -> Result<String> {
    use std::process::Command;
    let output = match Command::new("git").arg("ls-remote").arg(url).arg("HEAD").output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::msg("system `git` not found in PATH"));
        }
        Err(e) => return Err(Error::msg(format!("failed to spawn git: {e}"))),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::msg(format!("git ls-remote failed: {}", stderr.trim())));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output: "<sha>\tHEAD\n"
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "HEAD" {
            let sha = parts[0];
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(sha.to_string());
            }
        }
    }
    Err(Error::msg("could not parse git ls-remote output"))
}

fn clone_repo(url: &str, dest: &Path, ref_or_sha: Option<&str>) -> Result<()> {
    // libgit2 here was built without libssh2 — skip it for SSH URLs.
    if is_ssh_url(url) {
        if dest.exists() {
            std::fs::remove_dir_all(dest).ok();
        }
        return shell_git_clone(url, dest, ref_or_sha);
    }
    match clone_repo_libgit2(url, dest, ref_or_sha) {
        Ok(()) => Ok(()),
        Err(libgit2_err) => {
            // libgit2 may have left a partial directory; clean before fallback.
            if dest.exists() {
                std::fs::remove_dir_all(dest).ok();
            }
            match shell_git_clone(url, dest, ref_or_sha) {
                Ok(()) => Ok(()),
                Err(shell_err) => Err(Error::msg(format!(
                    "git clone failed: {libgit2_err}\nfallback: {shell_err}"
                ))),
            }
        }
    }
}

fn clone_repo_libgit2(url: &str, dest: &Path, ref_or_sha: Option<&str>) -> Result<()> {
    let mut builder = RepoBuilder::new();
    builder.fetch_options(make_fetch_options());
    if let Some(r) = ref_or_sha {
        // Only useful for branch refs; SHA handled separately below.
        builder.branch(r);
    }
    let repo = builder.clone(url, dest)?;
    drop(repo);
    Ok(())
}

/// Fall back to system `git clone` — uses the user's full git environment.
fn shell_git_clone(url: &str, dest: &Path, ref_or_sha: Option<&str>) -> Result<()> {
    use std::process::Command;
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(r) = ref_or_sha {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(url).arg(dest);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::msg("system `git` not found in PATH"));
        }
        Err(e) => return Err(Error::msg(format!("failed to spawn git: {e}"))),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::msg(format!("git clone failed: {}", stderr.trim())));
    }
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

fn is_ssh_url(url: &str) -> bool {
    url.starts_with("git@") || url.starts_with("ssh://")
}

fn derive_source_name(url: &str) -> String {
    let s = url.trim().trim_end_matches('/').trim_end_matches(".git");

    // SSH form: user@host:owner/repo → take owner/repo
    if let Some(rest) = s.strip_prefix("git@") {
        if let Some((_host, path)) = rest.split_once(':') {
            let path = path.trim_start_matches('/');
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 2 {
                return format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
            }
            return path.to_string();
        }
    }

    // ssh://, https://, http://, git://, plain "owner/repo": take last two segments
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
    let url = normalize_git_url(url);
    let identity = repo_identity(&url);

    // Check for duplicates: same repo already registered as source, or referenced
    // by a plugin entry in an existing marketplace source.
    if let Some(id) = &identity {
        for existing in list_sources() {
            if repo_identity(&existing.url).as_ref() == Some(id) {
                return Err(Error::msg(format!(
                    "Source already exists: '{}' ({})",
                    existing.name, existing.url
                )));
            }
            if let Ok(plugins) = parse_marketplace(&existing.name) {
                for p in plugins {
                    if let Some(spec) = p.source.as_object() {
                        let ref_url = match spec.get("source").and_then(|v| v.as_str()).unwrap_or("") {
                            "url" | "git-subdir" => spec.get("url").and_then(|v| v.as_str()).map(String::from),
                            "github" => spec.get("repo").and_then(|v| v.as_str()).map(|r| format!("https://github.com/{r}.git")),
                            _ => None,
                        };
                        if let Some(ref_url) = ref_url {
                            if repo_identity(&ref_url).as_ref() == Some(id) {
                                return Err(Error::msg(format!(
                                    "This repo is already referenced as plugin '{}' via source '{}'. Install it with: add {}",
                                    p.name, existing.name, p.name
                                )));
                            }
                        }
                    }
                }
            }
        }
    }

    let name = name.map(|s| s.to_string()).unwrap_or_else(|| derive_source_name(&url));
    let repo_dir = cache_dir_for(&name);
    std::fs::create_dir_all(cache_dir())?;
    if repo_dir.exists() {
        pull_ff(&repo_dir)?;
    } else {
        clone_repo(&url, &repo_dir, None)?;
    }
    let commit = git_head(&repo_dir)?;
    let src = Source {
        name: name.clone(),
        url: url.clone(),
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
    let url = url.trim();
    // Short form: "owner/repo" → GitHub HTTPS URL
    if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("git@") && !url.starts_with("ssh://") {
        return format!("https://github.com/{url}.git");
    }

    // GitHub web URLs: strip /tree/..., /blob/..., /releases, /pulls, /issues, etc.
    // Keep only https://<host>/<owner>/<repo> and ensure it ends with .git
    if let Some(rest) = url.strip_prefix("https://github.com/").or_else(|| url.strip_prefix("http://github.com/")) {
        // rest is like "owner/repo" or "owner/repo/tree/main" etc.
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            let repo = parts[1].trim_end_matches(".git");
            return format!("https://github.com/{}/{}.git", parts[0], repo);
        }
    }

    // Already a clone-ready URL; ensure .git suffix for https to improve consistency
    url.to_string()
}

/// Extract a canonical identity key from any git URL form.
/// Returns lowercase "host/owner/repo" (without .git, .git suffix, or protocol).
/// Returns None if the URL can't be parsed as a recognizable git URL.
fn repo_identity(url: &str) -> Option<String> {
    let url = url.trim();
    // git@host:owner/repo.git → host/owner/repo
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            let path = path.trim_start_matches('/').trim_end_matches(".git");
            return Some(format!("{}/{}", host.to_lowercase(), path.to_lowercase()));
        }
    }
    // ssh://git@host/owner/repo.git or https://host/owner/repo(.git)
    let rest = url
        .strip_prefix("ssh://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("git://"));
    if let Some(rest) = rest {
        // Strip user@ if present (ssh://git@host/...)
        let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
        let rest = rest.trim_end_matches('/').trim_end_matches(".git");
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 && !parts[0].is_empty() && !parts[1].is_empty() && !parts[2].is_empty() {
            return Some(format!("{}/{}/{}", parts[0].to_lowercase(), parts[1].to_lowercase(), parts[2].to_lowercase()));
        }
    }
    None
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
