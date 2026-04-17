//! End-to-end integration test (mirror of tests/test_e2e.py).
//! Uses an isolated HOME to avoid touching user's real config.
//! Requires network for git clone.

use kiro_cc_core::{converter, models::{self, Scope}, registry, scanner, source};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

fn kiro_agents() -> Vec<PathBuf> {
    let d = models::kiro_home().join("agents");
    if !d.is_dir() { return vec![]; }
    std::fs::read_dir(&d).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect()
}
fn kiro_skills() -> Vec<PathBuf> {
    let d = models::kiro_home().join("skills");
    if !d.is_dir() { return vec![]; }
    std::fs::read_dir(&d).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect()
}
fn read_mcp() -> Value {
    let p = models::kiro_home().join("settings").join("mcp.json");
    if !p.exists() { return Value::Null; }
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

fn install(plugin_name: &str) {
    let mut found = None;
    for s in source::list_sources() {
        if let Ok(plugins) = source::parse_marketplace(&s.name) {
            if let Some(p) = plugins.into_iter().find(|x| x.name == plugin_name) {
                found = Some((s.name, p));
                break;
            }
        }
    }
    let (source_name, mut plugin) = found.expect("plugin not found in any source");

    if plugin.local_path.as_ref().is_none_or(|p| !p.is_dir()) && plugin.source.is_object() {
        let fetched = source::fetch_external_plugin(&plugin.name, &plugin.source).unwrap();
        plugin.local_path = fetched;
    }
    let lp = plugin.local_path.as_ref().expect("no local path");

    let components = scanner::scan_plugin(lp, plugin.skill_filter.as_deref()).unwrap();
    converter::set_scope(Scope::Global);
    let mut converted = Vec::new();
    for comp in components {
        if let Some(rec) = converter::convert_component(&comp, &plugin.name, &source_name).unwrap() {
            converted.push(rec);
        }
    }
    let commit = source::list_sources().into_iter().find(|s| s.name == source_name).map(|s| s.commit).unwrap_or_default();
    registry::add_installed(&plugin.name, &source_name, converted, &commit, "global").unwrap();
}

fn uninstall(plugin_name: &str) {
    let Some(inst) = registry::get_installed_plugin(plugin_name) else { return };
    converter::set_scope(Scope::from_str(&inst.scope));
    let comps = registry::remove_installed(plugin_name).unwrap();
    for c in comps {
        converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).unwrap();
    }
}

#[test]
#[ignore] // requires network; run with: cargo test -- --ignored
fn e2e_full_flow() {
    // Isolate HOME
    let test_home = tempfile::tempdir().expect("tempdir");
    std::env::set_var("HOME", test_home.path());
    models::ensure_dirs().unwrap();

    // --- add sources ---
    source::add_source("https://github.com/anthropics/claude-plugins-official", None).unwrap();
    source::add_source("https://github.com/anthropics/skills.git", None).unwrap();
    let sources = source::list_sources();
    assert_eq!(sources.len(), 2, "2 sources registered");
    let names: HashSet<String> = sources.into_iter().map(|s| s.name).collect();
    assert!(names.contains("anthropics/claude-plugins-official"));
    assert!(names.contains("anthropics/skills"));

    // --- install feature-dev ---
    install("feature-dev");
    let agent_files: HashSet<String> = kiro_agents().iter().map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()).collect();
    for want in [
        "anthropics--claude-plugins-official--feature-dev--feature-dev",
        "anthropics--claude-plugins-official--feature-dev--code-reviewer",
        "anthropics--claude-plugins-official--feature-dev--code-explorer",
        "anthropics--claude-plugins-official--feature-dev--code-architect",
    ] {
        assert!(agent_files.contains(want), "missing {want}");
    }
    // verify internal name is unprefixed
    for (fname, inner) in [
        ("anthropics--claude-plugins-official--feature-dev--code-reviewer.json", "code-reviewer"),
        ("anthropics--claude-plugins-official--feature-dev--feature-dev.json", "feature-dev"),
    ] {
        let p = models::kiro_home().join("agents").join(fname);
        let d: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(d["name"].as_str().unwrap(), inner);
        assert!(d["description"].as_str().unwrap().contains("[from cc:feature-dev]"));
    }

    // --- install document-skills ---
    install("document-skills");
    let skill_names: HashSet<String> = kiro_skills().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    for want in [
        "anthropics--skills--document-skills--xlsx",
        "anthropics--skills--document-skills--pdf",
        "anthropics--skills--document-skills--pptx",
        "anthropics--skills--document-skills--docx",
    ] {
        assert!(skill_names.contains(want), "missing {want}");
        assert!(models::kiro_home().join("skills").join(want).join("SKILL.md").exists());
    }

    // --- install context7 (MCP) ---
    install("context7");
    let mcp = read_mcp();
    let servers = &mcp["mcpServers"];
    assert_eq!(servers["cc-context7-context7"]["command"].as_str().unwrap(), "npx");
    let args = servers["cc-context7-context7"]["args"].as_array().unwrap();
    assert!(args.iter().any(|v| v.as_str() == Some("@upstash/context7-mcp")));

    // --- registry state ---
    let installed = registry::get_installed();
    let names: HashSet<String> = installed.iter().map(|p| p.plugin_name.clone()).collect();
    assert_eq!(names, HashSet::from(["feature-dev".into(), "document-skills".into(), "context7".into()]));

    // --- uninstall all ---
    uninstall("feature-dev");
    uninstall("document-skills");
    uninstall("context7");

    assert_eq!(registry::get_installed().len(), 0);
    assert_eq!(kiro_agents().len(), 0);
    assert_eq!(kiro_skills().len(), 0);
    assert!(!read_mcp()["mcpServers"].as_object().unwrap().contains_key("cc-context7-context7"));
}
