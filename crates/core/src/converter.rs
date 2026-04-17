//! Convert Claude Code plugin components to Kiro format.

use crate::models::{
    kiro_root, load_json, registry_file, ComponentRecord, Error, Result, Scope,
};
use crate::scanner::ScannedComponent;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SCOPE: Mutex<Scope> = Mutex::new(Scope::Global);

pub fn set_scope(scope: Scope) {
    *SCOPE.lock().unwrap() = scope;
}

fn get_scope() -> Scope {
    *SCOPE.lock().unwrap()
}

// ── paths ────────────────────────────────────────────────────────────────

pub fn kiro_agent_path(name: &str, plugin_name: &str, source_name: &str) -> PathBuf {
    let filename = if !plugin_name.is_empty() && !source_name.is_empty() {
        let safe_source = source_name.replace('/', "--");
        format!("{safe_source}--{plugin_name}--{name}")
    } else {
        name.to_string()
    };
    kiro_root(get_scope()).join("agents").join(format!("{filename}.json"))
}

pub fn kiro_skill_dir(name: &str, plugin_name: &str, source_name: &str) -> PathBuf {
    let safe_source = source_name.replace('/', "--");
    kiro_root(get_scope())
        .join("skills")
        .join(format!("{safe_source}--{plugin_name}--{name}"))
}

/// Directory where disabled skills are parked.
pub fn disabled_skills_dir() -> PathBuf {
    crate::models::converter_home().join("disabled-skills")
}

pub fn disabled_agents_dir() -> PathBuf {
    crate::models::converter_home().join("disabled-agents")
}

/// Returns whether a converted component is currently enabled.
pub fn is_component_enabled(comp_type: &str, target_path: &str, mcp_keys: Option<&[String]>) -> bool {
    match comp_type {
        "skill" | "agent" | "command" => PathBuf::from(target_path).exists(),
        "mcp" => {
            let Some(keys) = mcp_keys else { return true };
            let p = PathBuf::from(target_path);
            if !p.exists() { return false; }
            let Ok(text) = std::fs::read_to_string(&p) else { return false };
            let Ok(v): std::result::Result<Value, _> = serde_json::from_str(&text) else { return false };
            let servers = v.get("mcpServers").and_then(|x| x.as_object());
            keys.iter().any(|k| {
                servers.and_then(|m| m.get(k))
                    .and_then(|s| s.get("disabled"))
                    .and_then(|d| d.as_bool())
                    .map(|d| !d)
                    .unwrap_or(true)
            })
        }
        _ => true,
    }
}

pub fn set_component_enabled(comp_type: &str, target_path: &str, mcp_keys: Option<&[String]>, enable: bool) -> Result<()> {
    match comp_type {
        "skill" => set_dir_enabled(target_path, &disabled_skills_dir(), enable),
        "agent" | "command" => set_file_enabled(target_path, &disabled_agents_dir(), enable),
        "mcp" => {
            let Some(keys) = mcp_keys else { return Ok(()); };
            let p = PathBuf::from(target_path);
            if !p.exists() { return Ok(()); }
            let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&p)?).unwrap_or(json!({"mcpServers": {}}));
            if let Some(m) = v.get_mut("mcpServers").and_then(|x| x.as_object_mut()) {
                for k in keys {
                    if let Some(server) = m.get_mut(k).and_then(|s| s.as_object_mut()) {
                        if enable { server.remove("disabled"); }
                        else { server.insert("disabled".into(), Value::Bool(true)); }
                    }
                }
            }
            write_json_file(&p, &v)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn set_dir_enabled(target_path: &str, parked_root: &Path, enable: bool) -> Result<()> {
    let active = PathBuf::from(target_path);
    let skill_dir = active.parent().ok_or_else(|| Error::msg("no parent"))?;
    let dir_name = skill_dir.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::msg("no dir name"))?;
    let parked = parked_root.join(dir_name);
    std::fs::create_dir_all(parked_root)?;
    if enable {
        if skill_dir.is_dir() { return Ok(()); }
        if parked.is_dir() { std::fs::rename(&parked, skill_dir)?; }
    } else {
        if !skill_dir.is_dir() { return Ok(()); }
        if parked.exists() { std::fs::remove_dir_all(&parked)?; }
        std::fs::rename(skill_dir, &parked)?;
    }
    Ok(())
}

fn set_file_enabled(target_path: &str, parked_root: &Path, enable: bool) -> Result<()> {
    let active = PathBuf::from(target_path);
    let file_name = active.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::msg("no file"))?;
    let parked = parked_root.join(file_name);
    std::fs::create_dir_all(parked_root)?;
    if enable {
        if active.exists() { return Ok(()); }
        if parked.exists() { std::fs::rename(&parked, &active)?; }
    } else {
        if !active.exists() { return Ok(()); }
        if parked.exists() { std::fs::remove_file(&parked)?; }
        std::fs::rename(&active, &parked)?;
    }
    Ok(())
}

pub fn kiro_mcp_settings_path() -> PathBuf {
    kiro_root(get_scope()).join("settings").join("mcp.json")
}

fn mcp_server_key(server_name: &str, plugin_name: &str, _source_name: &str) -> String {
    format!("cc-{plugin_name}-{server_name}")
}

// ── conflict checks ──────────────────────────────────────────────────────

#[derive(Debug, Default, serde::Deserialize)]
struct RegistryFile {
    #[serde(default)]
    installed: Vec<Value>,
}

fn is_ours(target: &Path) -> bool {
    if !target.exists() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(target) else {
        return false;
    };
    let Ok(d): std::result::Result<Value, _> = serde_json::from_str(&text) else {
        return false;
    };
    d.get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.contains("[from cc:"))
        .unwrap_or(false)
}

fn is_our_skill(target_dir: &Path) -> bool {
    let reg: RegistryFile = load_json(&registry_file()).unwrap_or_default();
    let target_str = target_dir.to_string_lossy();
    for p in &reg.installed {
        if let Some(comps) = p.get("components").and_then(|v| v.as_array()) {
            for c in comps {
                if let Some(tp) = c.get("target_path").and_then(|v| v.as_str()) {
                    if tp.contains(target_str.as_ref()) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn check_conflict(
    name: &str,
    comp_type: &str,
    plugin_name: &str,
    source_name: &str,
) -> Option<String> {
    match comp_type {
        "agent" | "command" => {
            let target = kiro_agent_path(name, plugin_name, source_name);
            if target.exists() && !is_ours(&target) {
                return Some(format!("Agent '{name}' already exists at {}", target.display()));
            }
        }
        "mcp" => {}
        "skill" => {
            let target = kiro_skill_dir(name, plugin_name, source_name);
            if target.is_dir() && !is_our_skill(&target) {
                return Some(format!("Skill '{name}' already exists at {}", target.display()));
            }
        }
        _ => {}
    }
    None
}

// ── tool mapping ─────────────────────────────────────────────────────────

fn map_tools(fm: &Value) -> Vec<String> {
    let raw = fm.get("tools");
    let tools: Vec<String> = match raw {
        Some(Value::String(s)) if !s.is_empty() => {
            s.split(',').map(|t| t.trim().to_string()).collect()
        }
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => return vec!["*".into()],
    };

    let mapping: &[(&str, &str)] = &[
        ("Read", "fs_read"),
        ("LS", "fs_read"),
        ("NotebookRead", "fs_read"),
        ("Write", "fs_write"),
        ("Edit", "fs_write"),
        ("NotebookEdit", "fs_write"),
        ("TodoWrite", "fs_write"),
        ("Glob", "glob"),
        ("Grep", "grep"),
        ("Bash", "execute_bash"),
        ("BashOutput", "execute_bash"),
        ("KillShell", "execute_bash"),
        ("Monitor", "execute_bash"),
        ("PowerShell", "execute_bash"),
        ("WebFetch", "web_fetch"),
        ("WebSearch", "web_search"),
        ("Agent", "use_subagent"),
        ("LSP", "code"),
    ];

    let mut out: std::collections::BTreeSet<String> = Default::default();
    for t in tools {
        let base = t.split('(').next().unwrap_or(&t).trim();
        if let Some((_, mapped)) = mapping.iter().find(|(k, _)| *k == base) {
            out.insert((*mapped).into());
        } else {
            out.insert(base.to_lowercase());
        }
    }
    if out.is_empty() {
        vec!["*".into()]
    } else {
        out.into_iter().collect()
    }
}

// ── converters ───────────────────────────────────────────────────────────

fn write_json_file(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{text}\n"))?;
    Ok(())
}

pub fn convert_agent(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<ComponentRecord> {
    let fm = &comp.frontmatter;

    let original_desc = fm.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let description = if original_desc.is_empty() {
        format!("[from cc:{plugin_name}]")
    } else {
        format!("{original_desc} [from cc:{plugin_name}]")
    };

    let mut cfg = Map::new();
    cfg.insert(
        "$schema".into(),
        Value::String(
            "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json"
                .into(),
        ),
    );
    cfg.insert("name".into(), Value::String(comp.name.clone()));
    cfg.insert("description".into(), Value::String(description));
    cfg.insert("prompt".into(), Value::String(comp.body.trim().to_string()));
    cfg.insert(
        "tools".into(),
        Value::Array(map_tools(fm).into_iter().map(Value::String).collect()),
    );
    cfg.insert("allowedTools".into(), Value::Array(vec![]));
    cfg.insert("resources".into(), Value::Array(vec![]));
    cfg.insert("hooks".into(), Value::Object(Map::new()));
    cfg.insert("toolsSettings".into(), Value::Object(Map::new()));

    if let Some(model) = fm.get("model").and_then(|v| v.as_str()) {
        let mapped = match model {
            "opus" => "claude-opus-4.7",
            "sonnet" => "claude-sonnet-4.6",
            "haiku" => "claude-haiku-4.5",
            other => other,
        };
        cfg.insert("model".into(), Value::String(mapped.into()));
    }

    let target = kiro_agent_path(&comp.name, plugin_name, source_name);
    write_json_file(&target, &Value::Object(cfg))?;
    Ok(ComponentRecord {
        component_type: comp.component_type.clone(),
        name: comp.name.clone(),
        source_rel: comp.rel_path.clone(),
        target_path: target.to_string_lossy().to_string(),
        mcp_keys: None,
    })
}

pub fn convert_skill(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<ComponentRecord> {
    let target_dir = kiro_skill_dir(&comp.name, plugin_name, source_name);
    std::fs::create_dir_all(&target_dir)?;

    let src_dir = comp.path.parent().ok_or_else(|| Error::msg("no parent dir"))?;
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let from = entry.path();
        let dest = target_dir.join(entry.file_name());
        if from.is_dir() {
            if dest.exists() {
                std::fs::remove_dir_all(&dest)?;
            }
            copy_dir_recursive(&from, &dest)?;
        } else {
            std::fs::copy(&from, &dest)?;
        }
    }

    let target = target_dir.join("SKILL.md");
    Ok(ComponentRecord {
        component_type: "skill".into(),
        name: comp.name.clone(),
        source_rel: comp.rel_path.clone(),
        target_path: target.to_string_lossy().to_string(),
        mcp_keys: None,
    })
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

pub fn convert_mcp(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<ComponentRecord> {
    // If wrapped in { "mcpServers": {...} }, unwrap
    let mut mcp_data = comp.frontmatter.clone();
    if let Some(servers) = mcp_data.get("mcpServers").cloned() {
        if servers.is_object() {
            mcp_data = servers;
        }
    }

    let mcp_path = kiro_mcp_settings_path();
    std::fs::create_dir_all(mcp_path.parent().unwrap())?;

    let mut settings: Value = if mcp_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&mcp_path)?).unwrap_or(json!({"mcpServers": {}}))
    } else {
        json!({"mcpServers": {}})
    };
    if !settings.get("mcpServers").is_some_and(|v| v.is_object()) {
        settings["mcpServers"] = json!({});
    }

    let mut added_keys = Vec::new();
    if let Some(servers) = mcp_data.as_object() {
        for (server_name, server_config) in servers {
            let Some(cfg_obj) = server_config.as_object() else {
                continue;
            };
            let mut kiro_server = Map::new();
            let stype = cfg_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match stype {
                "http" | "streamable-http" | "sse" => {
                    let t = if stype == "http" { "streamable-http" } else { stype };
                    kiro_server.insert("type".into(), Value::String(t.into()));
                    for key in ["url", "headers", "timeout"] {
                        if let Some(v) = cfg_obj.get(key) {
                            kiro_server.insert(key.into(), v.clone());
                        }
                    }
                }
                _ => {
                    for key in ["command", "args", "env", "timeout"] {
                        if let Some(v) = cfg_obj.get(key) {
                            kiro_server.insert(key.into(), v.clone());
                        }
                    }
                }
            }
            let full_key = mcp_server_key(server_name, plugin_name, source_name);
            settings["mcpServers"][full_key.clone()] = Value::Object(kiro_server);
            added_keys.push(full_key);
        }
    }

    write_json_file(&mcp_path, &settings)?;

    Ok(ComponentRecord {
        component_type: "mcp".into(),
        name: format!("{plugin_name}-mcp"),
        source_rel: ".mcp.json".into(),
        target_path: mcp_path.to_string_lossy().to_string(),
        mcp_keys: Some(added_keys),
    })
}

pub fn convert_component(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<Option<ComponentRecord>> {
    let rec = match comp.component_type.as_str() {
        "skill" => convert_skill(comp, plugin_name, source_name)?,
        "command" | "agent" => convert_agent(comp, plugin_name, source_name)?,
        "mcp" => convert_mcp(comp, plugin_name, source_name)?,
        _ => return Ok(None),
    };
    Ok(Some(rec))
}

pub fn remove_converted(target_path: &str, mcp_keys: Option<&[String]>) -> Result<()> {
    let p = PathBuf::from(target_path);

    // MCP: remove keys from mcp.json, keep the file
    if let Some(keys) = mcp_keys {
        if p.file_name().map(|n| n == "mcp.json").unwrap_or(false) {
            if p.exists() {
                let mut settings: Value =
                    serde_json::from_str(&std::fs::read_to_string(&p)?).unwrap_or(json!({"mcpServers": {}}));
                if let Some(m) = settings.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                    for k in keys {
                        m.remove(k);
                    }
                }
                write_json_file(&p, &settings)?;
            }
            return Ok(());
        }
    }

    if p.is_dir() {
        std::fs::remove_dir_all(&p).ok();
    } else if p.exists() {
        let parent = p.parent().map(|x| x.to_path_buf());
        std::fs::remove_file(&p).ok();
        // Skills: remove the whole skill dir (parent named skills/<...>)
        if let Some(parent) = parent {
            if parent.parent().and_then(|x| x.file_name()).map(|n| n == "skills").unwrap_or(false) {
                std::fs::remove_dir_all(&parent).ok();
            }
        }
    }
    Ok(())
}
