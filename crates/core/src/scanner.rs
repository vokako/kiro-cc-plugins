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

    // Skills: skills/*/SKILL.md
    let skills_dir = plugin_dir.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(allowed_set) = &allowed {
                if let Ok(canon) = path.canonicalize() {
                    if !allowed_set.contains(&canon) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let text = std::fs::read_to_string(&skill_md)?;
                let (fm, body) = parse_frontmatter(&text);
                let name = fm
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().into_owned());
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
            }
        }
    }

    // Commands: commands/*.md
    let cmds_dir = plugin_dir.join("commands");
    if cmds_dir.is_dir() {
        for entry in std::fs::read_dir(&cmds_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let (fm, body) = parse_frontmatter(&text);
            let name = fm
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into_owned());
            let rel_path = path.strip_prefix(plugin_dir).unwrap_or(&path).to_string_lossy().to_string();
            components.push(ScannedComponent {
                component_type: "command".into(),
                name,
                path,
                rel_path,
                frontmatter: fm,
                body,
            });
        }
    }

    // Agents: agents/*.md
    let agents_dir = plugin_dir.join("agents");
    if agents_dir.is_dir() {
        for entry in std::fs::read_dir(&agents_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let (fm, body) = parse_frontmatter(&text);
            let name = fm
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into_owned());
            let rel_path = path.strip_prefix(plugin_dir).unwrap_or(&path).to_string_lossy().to_string();
            components.push(ScannedComponent {
                component_type: "agent".into(),
                name,
                path,
                rel_path,
                frontmatter: fm,
                body,
            });
        }
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
