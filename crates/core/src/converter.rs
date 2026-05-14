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

    // For agent JSON files, also move the companion .md prompt file
    let companion_md = if active.extension().and_then(|e| e.to_str()) == Some("json") {
        Some(active.with_extension("md"))
    } else {
        None
    };
    let parked_md = companion_md.as_ref().map(|p| {
        parked_root.join(p.file_name().unwrap())
    });

    if enable {
        if active.exists() { return Ok(()); }
        if parked.exists() { std::fs::rename(&parked, &active)?; }
        if let (Some(cmd), Some(pmd)) = (&companion_md, &parked_md) {
            if pmd.exists() && !cmd.exists() {
                std::fs::rename(pmd, cmd)?;
            }
        }
    } else {
        if !active.exists() { return Ok(()); }
        if parked.exists() { std::fs::remove_file(&parked)?; }
        std::fs::rename(&active, &parked)?;
        if let (Some(cmd), Some(pmd)) = (&companion_md, &parked_md) {
            if cmd.exists() {
                if pmd.exists() { std::fs::remove_file(pmd)?; }
                std::fs::rename(cmd, pmd)?;
            }
        }
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

// ── hooks conversion ──────────────────────────────────────────────────────

/// Map a Claude Code hook event name to Kiro's hook event name.
fn map_hook_event(cc_event: &str) -> Option<&'static str> {
    match cc_event {
        "PreToolUse" => Some("preToolUse"),
        "PostToolUse" => Some("postToolUse"),
        "Stop" => Some("stop"),
        "UserPromptSubmit" => Some("userPromptSubmit"),
        "SessionStart" => Some("agentSpawn"),
        _ => None,
    }
}

/// Map a Claude Code tool matcher to Kiro tool name.
fn map_hook_matcher(cc_matcher: &str) -> String {
    let mapping: &[(&str, &str)] = &[
        ("Bash", "execute_bash"),
        ("Edit", "fs_write"),
        ("Write", "fs_write"),
        ("Read", "fs_read"),
        ("Glob", "glob"),
        ("Grep", "grep"),
    ];
    // Handle "Edit|Write" style matchers
    let parts: Vec<&str> = cc_matcher.split('|').collect();
    let mapped: Vec<String> = parts
        .iter()
        .map(|p| {
            let trimmed = p.trim();
            mapping
                .iter()
                .find(|(k, _)| *k == trimmed)
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| trimmed.to_lowercase())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    mapped.join("|")
}

/// Convert Claude Code hooks JSON (from hooks.json or agent frontmatter) to Kiro hooks format.
/// Returns a JSON object like: {"preToolUse": [...], "postToolUse": [...], ...}
pub fn convert_hooks(cc_hooks: &Value, plugin_dir: Option<&Path>) -> Value {
    let Some(hooks_obj) = cc_hooks.as_object() else {
        return Value::Object(Map::new());
    };

    let mut kiro_hooks: Map<String, Value> = Map::new();

    for (event_name, groups) in hooks_obj {
        let Some(kiro_event) = map_hook_event(event_name) else {
            continue;
        };
        let Some(groups_arr) = groups.as_array() else {
            continue;
        };

        let mut kiro_entries: Vec<Value> = Vec::new();

        for group in groups_arr {
            let matcher = group.get("matcher").and_then(|v| v.as_str()).unwrap_or("*");
            let Some(handlers) = group.get("hooks").and_then(|v| v.as_array()) else {
                continue;
            };

            for handler in handlers {
                // Only convert command-type hooks
                let hook_type = handler.get("type").and_then(|v| v.as_str()).unwrap_or("command");
                if hook_type != "command" {
                    continue;
                }
                let Some(command) = handler.get("command").and_then(|v| v.as_str()) else {
                    continue;
                };

                // Replace ${CLAUDE_PLUGIN_ROOT} with actual plugin dir if available
                let resolved_command = if let Some(dir) = plugin_dir {
                    command
                        .replace("${CLAUDE_PLUGIN_ROOT}", &dir.to_string_lossy())
                        .replace("$CLAUDE_PLUGIN_ROOT", &dir.to_string_lossy())
                } else {
                    command.to_string()
                };

                let mut entry = Map::new();
                entry.insert("command".into(), Value::String(resolved_command));

                // Add matcher if not wildcard
                if matcher != "*" && !matcher.is_empty() {
                    entry.insert("matcher".into(), Value::String(map_hook_matcher(matcher)));
                }

                // Add timeout if specified
                if let Some(timeout) = handler.get("timeout").and_then(|v| v.as_u64()) {
                    entry.insert("timeout_ms".into(), Value::Number((timeout * 1000).into()));
                }

                kiro_entries.push(Value::Object(entry));
            }
        }

        if !kiro_entries.is_empty() {
            kiro_hooks.insert(kiro_event.into(), Value::Array(kiro_entries));
        }
    }

    Value::Object(kiro_hooks)
}

/// Load and convert hooks from a plugin's hooks/hooks.json file.
pub fn load_plugin_hooks(plugin_dir: &Path) -> Value {
    let hooks_file = plugin_dir.join("hooks").join("hooks.json");
    if !hooks_file.exists() {
        return Value::Object(Map::new());
    }
    let Ok(text) = std::fs::read_to_string(&hooks_file) else {
        return Value::Object(Map::new());
    };
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return Value::Object(Map::new());
    };
    let hooks_val = data.get("hooks").unwrap_or(&data);
    convert_hooks(hooks_val, Some(plugin_dir))
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

    // Derive plugin directory from component path
    // comp.path is like <plugin_dir>/agents/foo.md or <plugin_dir>/commands/foo.md
    let plugin_dir = comp.path.parent().and_then(|p| p.parent());

    // Convert hooks from agent frontmatter
    let mut hooks = if let Some(hooks_val) = fm.get("hooks") {
        convert_hooks(hooks_val, plugin_dir)
    } else {
        Value::Object(Map::new())
    };

    // Also load plugin-level hooks/hooks.json and merge
    if let Some(pd) = plugin_dir {
        let plugin_hooks = load_plugin_hooks(pd);
        if let (Some(target), Some(source)) = (hooks.as_object_mut(), plugin_hooks.as_object()) {
            for (k, v) in source {
                if let Some(existing) = target.get_mut(k) {
                    // Merge arrays
                    if let (Some(arr), Some(new_arr)) = (existing.as_array_mut(), v.as_array()) {
                        arr.extend(new_arr.clone());
                    }
                } else {
                    target.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // Write prompt to a separate .md file and reference it
    let target = kiro_agent_path(&comp.name, plugin_name, source_name);
    let prompt_path = target.with_extension("md");
    let prompt_content = comp.body.trim();
    if let Some(parent) = prompt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&prompt_path, format!("{prompt_content}\n"))?;
    let prompt_filename = prompt_path.file_name().unwrap().to_string_lossy().to_string();

    // Display name: namespace commands as "{plugin}:{name}" so users invoke them
    // the same way as in Claude Code (e.g. "superpowers:brainstorming").
    // Subagents (from agents/) keep their original name — they're internal helpers
    // Claude spawns, not user-facing entry points.
    let display_name = if comp.component_type == "command" {
        format!("{plugin_name}:{}", comp.name)
    } else {
        comp.name.clone()
    };

    let mut cfg = Map::new();
    cfg.insert(
        "$schema".into(),
        Value::String(
            "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json"
                .into(),
        ),
    );
    cfg.insert("name".into(), Value::String(display_name.clone()));
    cfg.insert("description".into(), Value::String(description));
    cfg.insert("prompt".into(), Value::String(format!("file://./{prompt_filename}")));
    cfg.insert(
        "tools".into(),
        Value::Array(map_tools(fm).into_iter().map(Value::String).collect()),
    );
    cfg.insert("allowedTools".into(), Value::Array(vec![]));
    // Agents in Claude Code are isolated subagents — don't inject global resources/MCP.
    // Commands run as main-thread agents in Kiro, so they should access global resources.
    let is_subagent = comp.component_type == "agent";
    if is_subagent {
        cfg.insert("resources".into(), Value::Array(vec![]));
    } else {
        let kiro = crate::models::kiro_home();
        cfg.insert(
            "resources".into(),
            Value::Array(vec![
                // Global (absolute path)
                Value::String(format!("file://{}/steering/**/*.md", kiro.display())),
                Value::String(format!("skill://{}/skills/**/SKILL.md", kiro.display())),
                // Workspace (relative path, resolved from cwd)
                Value::String("file://.kiro/steering/**/*.md".into()),
                Value::String("skill://.kiro/skills/**/SKILL.md".into()),
            ]),
        );
        cfg.insert("includeMcpJson".into(), Value::Bool(true));
    }
    cfg.insert("hooks".into(), hooks);
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

    write_json_file(&target, &Value::Object(cfg))?;
    Ok(ComponentRecord {
        component_type: comp.component_type.clone(),
        name: display_name,
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
            // Preserve 'disabled' flag if user had disabled this server previously
            if let Some(existing) = settings.get("mcpServers").and_then(|v| v.get(&full_key)) {
                if let Some(disabled) = existing.get("disabled") {
                    kiro_server.insert("disabled".into(), disabled.clone());
                }
            }
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

/// Create a Kiro skill from a Claude Code command. Commands are user-invocable
/// in Claude Code; by also exposing them as skills, Kiro can auto-trigger them
/// when the user's intent matches the command's description.
fn convert_command_to_skill(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<ComponentRecord> {
    let target_dir = kiro_skill_dir(&comp.name, plugin_name, source_name);
    std::fs::create_dir_all(&target_dir)?;

    let fm = &comp.frontmatter;
    let description = fm.get("description").and_then(|v| v.as_str()).unwrap_or("").trim();
    let body = comp.body.trim();

    // Namespace the skill name to match Claude Code's "{plugin}:{name}" convention.
    let display_name = format!("{plugin_name}:{}", comp.name);

    // Build SKILL.md with YAML frontmatter Kiro expects (name + description required)
    let skill_md = format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n",
        display_name,
        description,
        body,
    );

    let target = target_dir.join("SKILL.md");
    std::fs::write(&target, skill_md)?;

    Ok(ComponentRecord {
        component_type: "skill".into(),
        name: display_name,
        source_rel: comp.rel_path.clone(),
        target_path: target.to_string_lossy().to_string(),
        mcp_keys: None,
    })
}

pub fn convert_component(
    comp: &ScannedComponent,
    plugin_name: &str,
    source_name: &str,
) -> Result<Vec<ComponentRecord>> {
    let recs = match comp.component_type.as_str() {
        "skill" => vec![convert_skill(comp, plugin_name, source_name)?],
        "command" => {
            // Commands become both agents (for explicit invocation) and skills (for auto-trigger)
            let agent_rec = convert_agent(comp, plugin_name, source_name)?;
            let skill_rec = convert_command_to_skill(comp, plugin_name, source_name)?;
            vec![agent_rec, skill_rec]
        }
        "agent" => vec![convert_agent(comp, plugin_name, source_name)?],
        "mcp" => vec![convert_mcp(comp, plugin_name, source_name)?],
        _ => return Ok(Vec::new()),
    };
    Ok(recs)
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
        // Agents: also remove the companion .md prompt file
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            let prompt_file = p.with_extension("md");
            if prompt_file.exists() {
                std::fs::remove_file(&prompt_file).ok();
            }
        }
        // Skills: remove the whole skill dir (parent named skills/<...>)
        if let Some(parent) = parent {
            if parent.parent().and_then(|x| x.file_name()).map(|n| n == "skills").unwrap_or(false) {
                std::fs::remove_dir_all(&parent).ok();
            }
        }
    } else {
        // File not at active location — check if it was disabled (parked elsewhere)
        if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
            // Check disabled-agents dir
            let parked_agent = disabled_agents_dir().join(file_name);
            if parked_agent.exists() {
                std::fs::remove_file(&parked_agent).ok();
                // companion .md
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    let parked_md = parked_agent.with_extension("md");
                    if parked_md.exists() {
                        std::fs::remove_file(&parked_md).ok();
                    }
                }
            }
            // Check disabled-skills dir (only for skill SKILL.md paths)
            if let Some(parent) = p.parent() {
                if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                    let parked_skill = disabled_skills_dir().join(dir_name);
                    if parked_skill.is_dir() {
                        std::fs::remove_dir_all(&parked_skill).ok();
                    }
                }
            }
        }
    }
    Ok(())
}
