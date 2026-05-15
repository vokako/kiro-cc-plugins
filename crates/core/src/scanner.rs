//! Scan Claude Code plugin directories for convertible components.

use crate::models::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ScannedComponent {
    pub component_type: String, // skill | command | agent | mcp
    pub name: String,
    pub path: PathBuf,
    pub rel_path: String,
    pub frontmatter: Value,
    pub body: String,
}

/// An unsupported component that was detected but cannot be converted.
#[derive(Debug, Clone)]
pub struct SkippedComponent {
    pub component_type: String, // hooks | hooks-handlers | lsp | runtime
    pub name: String,
    pub reason: String,
}

fn parse_frontmatter(text: &str) -> (Value, String) {
    // Matches: ---\n<yaml>\n---\n<body>
    let bytes = text.as_bytes();
    if !text.starts_with("---") {
        return (Value::Null, text.to_string());
    }
    // find the closing ---
    let after_first = &text[3..];
    let after_first_trim_start = after_first.trim_start_matches(|c: char| c == '\r' || c == '\n');
    let offset = text.len() - after_first_trim_start.len();
    let rest = &text[offset..];
    if let Some(end) = rest.find("\n---") {
        let yaml_str = &rest[..end];
        let body_start = end + 4;
        let body = rest[body_start..].trim_start_matches(|c: char| c == '\r' || c == '\n');
        let fm: Value = serde_yaml::from_str::<serde_yaml::Value>(yaml_str)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok())
            .unwrap_or(Value::Object(Default::default()));
        let _ = bytes;
        return (fm, body.to_string());
    }
    (Value::Null, text.to_string())
}

/// Scan a single skill directory (containing `SKILL.md` directly).
fn scan_skill_dir(
    skill_dir: &Path,
    plugin_dir: &Path,
    allowed: &Option<HashSet<PathBuf>>,
    components: &mut Vec<ScannedComponent>,
) -> Result<()> {
    if let Some(allowed_set) = allowed {
        if let Ok(canon) = skill_dir.canonicalize() {
            if !allowed_set.contains(&canon) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }
    let skill_md = skill_dir.join("SKILL.md");
    if !skill_md.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&skill_md)?;
    let (fm, body) = parse_frontmatter(&text);
    let name = fm
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| skill_dir.file_name().unwrap().to_string_lossy().into_owned());
    let rel_path = skill_md
        .strip_prefix(plugin_dir)
        .unwrap_or(&skill_md)
        .to_string_lossy()
        .to_string();
    components.push(ScannedComponent {
        component_type: "skill".into(),
        name,
        path: skill_md,
        rel_path,
        frontmatter: fm,
        body,
    });
    Ok(())
}

/// Scan a "skills root" — a directory that either (a) IS a skill (contains
/// SKILL.md directly) or (b) has each subdirectory as a separate skill.
fn scan_skills_root(
    skills_root: &Path,
    plugin_dir: &Path,
    allowed: &Option<HashSet<PathBuf>>,
    components: &mut Vec<ScannedComponent>,
) -> Result<()> {
    if !skills_root.is_dir() {
        return Ok(());
    }
    if skills_root.join("SKILL.md").exists() {
        scan_skill_dir(skills_root, plugin_dir, allowed, components)?;
    } else {
        for entry in std::fs::read_dir(skills_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                scan_skill_dir(&path, plugin_dir, allowed, components)?;
            }
        }
    }
    Ok(())
}

/// Scan markdown component files (commands or agents). The `path` may be
/// either a `.md` file directly or a directory containing `.md` files.
fn scan_md_components(
    path: &Path,
    plugin_dir: &Path,
    component_type: &str,
    components: &mut Vec<ScannedComponent>,
) -> Result<()> {
    let entries: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        std::fs::read_dir(path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect()
    } else {
        return Ok(());
    };
    for md in entries {
        let text = std::fs::read_to_string(&md)?;
        let (fm, body) = parse_frontmatter(&text);
        let name = fm
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| md.file_stem().unwrap().to_string_lossy().into_owned());
        let rel_path = md.strip_prefix(plugin_dir).unwrap_or(&md).to_string_lossy().to_string();
        components.push(ScannedComponent {
            component_type: component_type.into(),
            name,
            path: md,
            rel_path,
            frontmatter: fm,
            body,
        });
    }
    Ok(())
}

pub fn scan_plugin(plugin_dir: &Path, skill_filter: Option<&[String]>) -> Result<Vec<ScannedComponent>> {
    if !plugin_dir.is_dir() {
        return Ok(Vec::new());
    }

    let allowed: Option<HashSet<PathBuf>> = skill_filter.map(|filters| {
        filters
            .iter()
            .filter_map(|sf| {
                let rel = sf.trim_start_matches("./");
                let p = plugin_dir.join(rel);
                p.canonicalize().ok()
            })
            .collect()
    });

    let mut components = Vec::new();

    // Read plugin manifest if present — its component path overrides take precedence.
    let manifest: Option<Value> = std::fs::read_to_string(
        plugin_dir.join(".claude-plugin").join("plugin.json"),
    )
    .ok()
    .and_then(|t| serde_json::from_str(&t).ok());

    // Resolve a manifest field (string or array of strings) into absolute paths.
    let resolve_paths = |key: &str| -> Option<Vec<PathBuf>> {
        manifest.as_ref().and_then(|m| m.get(key)).and_then(|v| {
            let resolve = |s: &str| plugin_dir.join(s.trim_start_matches("./"));
            match v {
                Value::String(s) => Some(vec![resolve(s)]),
                Value::Array(arr) => Some(
                    arr.iter().filter_map(|x| x.as_str().map(resolve)).collect(),
                ),
                _ => None,
            }
        })
    };

    // Skills: manifest override OR default (skills/ + .claude/skills/ fallback).
    let skill_roots: Vec<PathBuf> = resolve_paths("skills").unwrap_or_else(|| {
        vec![
            plugin_dir.join("skills"),
            plugin_dir.join(".claude").join("skills"),
        ]
    });
    for root in &skill_roots {
        scan_skills_root(root, plugin_dir, &allowed, &mut components)?;
    }

    // Commands: manifest override OR default (commands/ + .claude/commands/).
    let cmd_paths: Vec<PathBuf> = resolve_paths("commands").unwrap_or_else(|| {
        vec![
            plugin_dir.join("commands"),
            plugin_dir.join(".claude").join("commands"),
        ]
    });
    for path in &cmd_paths {
        scan_md_components(path, plugin_dir, "command", &mut components)?;
    }

    // Agents: manifest override OR default (agents/ + .claude/agents/).
    let agent_paths: Vec<PathBuf> = resolve_paths("agents").unwrap_or_else(|| {
        vec![
            plugin_dir.join("agents"),
            plugin_dir.join(".claude").join("agents"),
        ]
    });
    for path in &agent_paths {
        scan_md_components(path, plugin_dir, "agent", &mut components)?;
    }

    // MCP: .mcp.json
    let mcp_file = plugin_dir.join(".mcp.json");
    if mcp_file.exists() {
        if let Ok(text) = std::fs::read_to_string(&mcp_file) {
            if let Ok(mcp_data) = serde_json::from_str::<Value>(&text) {
                components.push(ScannedComponent {
                    component_type: "mcp".into(),
                    name: "mcp".into(),
                    path: mcp_file,
                    rel_path: ".mcp.json".into(),
                    frontmatter: mcp_data,
                    body: String::new(),
                });
            }
        }
    }

    Ok(components)
}

/// Detect unsupported components in a plugin directory that cannot be converted.
pub fn scan_skipped(plugin_dir: &Path) -> Vec<SkippedComponent> {
    scan_skipped_with_meta(plugin_dir, None)
}

/// Detect unsupported components, optionally checking marketplace metadata for lspServers.
pub fn scan_skipped_with_meta(plugin_dir: &Path, marketplace_entry: Option<&Value>) -> Vec<SkippedComponent> {
    let mut skipped = Vec::new();

    // hooks/ directory
    if plugin_dir.join("hooks").is_dir() {
        // Check if hooks.json exists and has convertible command hooks
        let hooks_json = plugin_dir.join("hooks").join("hooks.json");
        let has_convertible = hooks_json.exists();
        let reason = if has_convertible {
            "Partially converted: command hooks injected into agents, but global hook scope not supported"
        } else {
            "Kiro has no hook system"
        };
        skipped.push(SkippedComponent {
            component_type: "hooks".into(),
            name: "hooks".into(),
            reason: reason.into(),
        });
    }

    // hooks.json at root level
    if plugin_dir.join("hooks.json").exists() {
        skipped.push(SkippedComponent {
            component_type: "hooks".into(),
            name: "hooks.json".into(),
            reason: "Kiro has no hook system".into(),
        });
    }

    // hooks-handlers/
    if plugin_dir.join("hooks-handlers").is_dir() {
        skipped.push(SkippedComponent {
            component_type: "hooks-handlers".into(),
            name: "hooks-handlers".into(),
            reason: "Kiro has no hook system".into(),
        });
    }

    // lspServers in plugin.json or marketplace entry
    let mut has_lsp = false;
    for fname in &[".claude-plugin/plugin.json", "plugin.json"] {
        let pj = plugin_dir.join(fname);
        if pj.exists() {
            if let Ok(text) = std::fs::read_to_string(&pj) {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    if data.get("lspServers").is_some() {
                        has_lsp = true;
                        break;
                    }
                }
            }
        }
    }
    if !has_lsp {
        if let Some(entry) = marketplace_entry {
            if entry.get("lspServers").is_some() {
                has_lsp = true;
            }
        }
    }
    if has_lsp {
        skipped.push(SkippedComponent {
            component_type: "lsp".into(),
            name: "lspServers".into(),
            reason: "Kiro does not support custom LSP configuration".into(),
        });
    }

    // Runtime code directories (core/, utils/, matchers/)
    for dir_name in &["core", "utils", "matchers"] {
        let d = plugin_dir.join(dir_name);
        if d.is_dir() {
            skipped.push(SkippedComponent {
                component_type: "runtime".into(),
                name: dir_name.to_string(),
                reason: "Runtime code dependency, not convertible".into(),
            });
        }
    }

    skipped
}
