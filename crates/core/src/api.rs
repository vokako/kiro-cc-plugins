//! JSON API dispatcher — shared between CLI `api` subcommand and GUI backend.

use crate::{converter, models::Scope, registry, scanner, source};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub fn dispatch(request: &Value) -> Value {
    if let Err(e) = crate::models::ensure_dirs() {
        return json!({"success": false, "error": format!("ensure_dirs: {e}")});
    }
    let cmd = request.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let args = request.get("args").cloned().unwrap_or(Value::Null);
    let result = match cmd {
        "source.list" => source_list(),
        "source.add" => source_add(args),
        "source.remove" => source_remove(args),
        "source.update" => source_update(args),
        "source.check_updates" => source_check_updates(),
        "plugin.list" => plugin_list(args),
        "plugin.detail" => plugin_detail(args),
        "plugin.add" => plugin_add(args),
        "plugin.delete" => plugin_delete(args),
        "plugin.update" => plugin_update(args),
        "plugin.toggle" => plugin_toggle(args),
        "config.export" => config_export(),
        "config.import" => config_import(args),
        "app.check_update" => app_check_update(args),
        other => Err(format!("Unknown command: {other}")),
    };
    match result {
        Ok(data) => json!({"success": true, "data": data}),
        Err(msg) => json!({"success": false, "error": msg}),
    }
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> { v.get(k).and_then(|x| x.as_str()) }
fn b(v: &Value, k: &str) -> bool { v.get(k).and_then(|x| x.as_bool()).unwrap_or(false) }
fn vs(v: &Value, k: &str) -> Option<Vec<String>> {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|i| i.as_str().map(String::from)).collect())
}
fn err_str<E: std::fmt::Display>(e: E) -> String { format!("{e}") }

// ── source ───────────────────────────────────────────────────────────────

fn source_list() -> Result<Value, String> {
    Ok(serde_json::to_value(source::list_sources()).map_err(err_str)?)
}

fn source_add(args: Value) -> Result<Value, String> {
    let url = s(&args, "url").ok_or("missing url")?;
    let name = s(&args, "name");
    let src = source::add_source(url, name).map_err(err_str)?;
    let plugins = source::parse_marketplace(&src.name).map_err(err_str)?;
    Ok(json!({"source": {"name": src.name, "url": src.url, "commit": src.commit}, "plugin_count": plugins.len()}))
}

fn source_remove(args: Value) -> Result<Value, String> {
    let force = b(&args, "force");
    let remove_all = b(&args, "all");
    let targets: Vec<String> = if remove_all {
        if !force { return Err("--all requires force=true".into()); }
        source::list_sources().into_iter().map(|s| s.name).collect()
    } else if let Some(n) = s(&args, "name") {
        vec![n.to_string()]
    } else {
        return Err("Specify a source name or all=true".into());
    };
    let mut results = Vec::new();
    for sn in targets {
        let installed: Vec<_> = registry::get_installed().into_iter().filter(|p| p.source_name == sn).collect();
        if !installed.is_empty() && !force {
            let names: Vec<&str> = installed.iter().map(|p| p.plugin_name.as_str()).collect();
            return Err(format!("Source '{sn}' has {} installed plugin(s): {}. Use force=true.", installed.len(), names.join(", ")));
        }
        for p in &installed {
            converter::set_scope(Scope::from_str(&p.scope));
            let comps = registry::remove_installed(&p.plugin_name).map_err(err_str)?;
            for c in &comps { converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(err_str)?; }
        }
        source::remove_source(&sn).map_err(err_str)?;
        let un: Vec<String> = installed.iter().map(|p| p.plugin_name.clone()).collect();
        results.push(json!({"name": sn, "uninstalled_plugins": un}));
    }
    Ok(Value::Array(results))
}

fn source_update(args: Value) -> Result<Value, String> {
    let targets: Vec<String> = if let Some(n) = s(&args, "name") {
        vec![n.to_string()]
    } else {
        source::list_sources().into_iter().map(|s| s.name).collect()
    };
    let mut results = Vec::new();
    for sn in targets {
        let commit = source::update_source(&sn).map_err(err_str)?;
        results.push(json!({"name": sn, "commit": commit}));
    }
    Ok(Value::Array(results))
}

// ── plugin helpers ───────────────────────────────────────────────────────

fn find_plugin(name: &str) -> Option<(String, source::MarketplacePlugin)> {
    for src in source::list_sources() {
        if let Ok(plugins) = source::parse_marketplace(&src.name) {
            if let Some(p) = plugins.into_iter().find(|p| p.name == name) {
                return Some((src.name.clone(), p));
            }
        }
    }
    None
}

fn plugin_status(p: &source::MarketplacePlugin, installed: &HashSet<String>) -> &'static str {
    if installed.contains(&p.name) { "installed" }
    else if p.local_path.as_ref().is_some_and(|lp| lp.is_dir()) { "available" }
    else if p.source.is_object() { "fetchable" }
    else { "unavailable" }
}

// ── plugin handlers ──────────────────────────────────────────────────────

fn plugin_list(args: Value) -> Result<Value, String> {
    let source_name = s(&args, "source");
    let installed_only = b(&args, "installed");
    let only_types: Option<HashSet<String>> = vs(&args, "only_types").map(|v| v.into_iter().collect());

    if installed_only {
        let items = registry::get_installed();
        let mut out = Vec::new();
        for p in items {
            converter::set_scope(Scope::from_str(&p.scope));
            let mut comps_json = Vec::new();
            for c in &p.components {
                if let Some(f) = &only_types { if !f.contains(&c.component_type) { continue; } }
                let enabled = converter::is_component_enabled(&c.component_type, &c.target_path, c.mcp_keys.as_deref());
                comps_json.push(json!({"type": c.component_type, "name": c.name, "enabled": enabled}));
            }
            out.push(json!({
                "plugin_name": p.plugin_name, "source_name": p.source_name,
                "scope": p.scope, "installed_at": p.installed_at, "components": comps_json,
            }));
        }
        return Ok(Value::Array(out));
    }

    let per_source: Vec<(String, Vec<source::MarketplacePlugin>)> = if let Some(sn) = source_name {
        vec![(sn.to_string(), source::parse_marketplace(sn).map_err(err_str)?)]
    } else {
        source::list_sources().into_iter()
            .filter_map(|src| source::parse_marketplace(&src.name).ok().map(|p| (src.name, p)))
            .collect()
    };

    let installed_names: HashSet<String> = registry::get_installed().into_iter().map(|p| p.plugin_name).collect();
    let mut result = Vec::new();
    for (sn, plugins) in per_source {
        for p in plugins {
            let status = plugin_status(&p, &installed_names);
            let mut entry = Map::new();
            entry.insert("name".into(), json!(p.name));
            entry.insert("description".into(), json!(p.description));
            entry.insert("category".into(), json!(p.category));
            entry.insert("source_name".into(), json!(sn));
            entry.insert("status".into(), json!(status));
            if let Some(lp) = &p.local_path {
                if lp.is_dir() {
                    let comps = scanner::scan_plugin(lp, p.skill_filter.as_deref()).map_err(err_str)?;
                    let cl: Vec<Value> = comps.into_iter()
                        .filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type)))
                        .map(|c| json!({"type": c.component_type, "name": c.name, "description": c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or("")}))
                        .collect();
                    entry.insert("components".into(), Value::Array(cl));
                    let lsp_meta = if p.has_lsp_servers { Some(&json!({"lspServers": true})) } else { None };
                    let sk: Vec<Value> = scanner::scan_skipped_with_meta(lp, lsp_meta).into_iter()
                        .map(|s| json!({"type": s.component_type, "name": s.name, "reason": s.reason}))
                        .collect();
                    if !sk.is_empty() {
                        entry.insert("skipped".into(), Value::Array(sk));
                    }
                }
            }
            result.push(Value::Object(entry));
        }
    }
    Ok(Value::Array(result))
}

fn plugin_detail(args: Value) -> Result<Value, String> {
    let name = s(&args, "name").ok_or("missing name")?;
    let (source_name, plugin) = find_plugin(name).ok_or_else(|| format!("Plugin '{name}' not found"))?;
    let installed = registry::get_installed_plugin(name);
    let mut components = Vec::new();
    let mut skipped = Vec::new();
    if let Some(lp) = &plugin.local_path {
        if lp.is_dir() {
            let comps = scanner::scan_plugin(lp, plugin.skill_filter.as_deref()).map_err(err_str)?;
            let inst_comps = installed.as_ref().map(|p| &p.components);
            components = comps.into_iter().map(|c| {
                let enabled = inst_comps.and_then(|ic| ic.iter().find(|rc| rc.name == c.name && rc.component_type == c.component_type))
                    .map(|rc| converter::is_component_enabled(&rc.component_type, &rc.target_path, rc.mcp_keys.as_deref()));
                let mut obj = json!({"type": c.component_type, "name": c.name, "description": c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or("")});
                if let Some(en) = enabled { obj["enabled"] = json!(en); }
                obj
            }).collect();
            let lsp_meta = if plugin.has_lsp_servers { Some(&json!({"lspServers": true})) } else { None };
            skipped = scanner::scan_skipped_with_meta(lp, lsp_meta).into_iter()
                .map(|s| json!({"type": s.component_type, "name": s.name, "reason": s.reason}))
                .collect();
        }
    }
    Ok(json!({"name": name, "description": plugin.description, "category": plugin.category, "source": source_name, "installed": installed.is_some(), "components": components, "skipped": skipped}))
}

fn plugin_add(args: Value) -> Result<Value, String> {
    let plugin_name = s(&args, "name");
    let mut source_name = s(&args, "source").map(String::from);
    let scope = s(&args, "scope").unwrap_or("global").to_string();
    let only_types: Option<HashSet<String>> = vs(&args, "only_types").map(|v| v.into_iter().collect());
    let install_all = b(&args, "all");
    converter::set_scope(Scope::from_str(&scope));

    if let (Some(pn), None) = (plugin_name, source_name.as_deref()) {
        for src in source::list_sources() {
            if let Ok(plugins) = source::parse_marketplace(&src.name) {
                if plugins.iter().any(|p| p.name == pn) { source_name = Some(src.name.clone()); break; }
            }
        }
    }
    let source_name = match source_name {
        Some(s) => s,
        None => {
            let sources = source::list_sources();
            if sources.len() == 1 { sources[0].name.clone() }
            else if plugin_name.is_some() { return Err(format!("Plugin '{}' not found", plugin_name.unwrap())); }
            else { return Err("Multiple sources. Specify source.".into()); }
        }
    };
    let plugins = source::parse_marketplace(&source_name).map_err(err_str)?;
    let commit = source::list_sources().into_iter().find(|s| s.name == source_name).map(|s| s.commit).unwrap_or_default();
    let targets: Vec<source::MarketplacePlugin> = if install_all {
        plugins.into_iter().filter(|p| p.local_path.as_ref().is_some_and(|lp| lp.is_dir())).collect()
    } else if let Some(pn) = plugin_name {
        plugins.into_iter().filter(|p| p.name == pn).collect()
    } else { return Err("Specify a plugin name or all=true".into()); };

    let mut results = Vec::new();
    for plugin in targets {
        let local_path = match &plugin.local_path {
            Some(lp) if lp.is_dir() => Some(lp.clone()),
            _ if plugin.source.is_object() => {
                match source::fetch_external_plugin(&plugin.name, &plugin.source).map_err(err_str)? {
                    Some(p) if p.is_dir() => Some(p), _ => { results.push(json!({"name": plugin.name, "status": "fetch_failed"})); continue; }
                }
            }
            _ => { results.push(json!({"name": plugin.name, "status": "unavailable"})); continue; }
        };
        let lp = local_path.unwrap();
        let comps = scanner::scan_plugin(&lp, plugin.skill_filter.as_deref()).map_err(err_str)?;
        let comps: Vec<_> = comps.into_iter().filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type))).collect();
        let mut converted = Vec::new();
        let mut skipped = Vec::new();
        for comp in comps {
            if let Some(conflict) = converter::check_conflict(&comp.name, &comp.component_type, &plugin.name, &source_name) {
                skipped.push(json!({"type": comp.component_type, "name": comp.name, "reason": conflict})); continue;
            }
            let recs = converter::convert_component(&comp, &plugin.name, &source_name).map_err(err_str)?;
            converted.extend(recs);
        }
        let n = converted.len();
        if !converted.is_empty() { registry::add_installed(&plugin.name, &source_name, converted, &commit, &scope).map_err(err_str)?; }
        results.push(json!({"name": plugin.name, "status": "installed", "components": n, "skipped": skipped}));
    }
    Ok(Value::Array(results))
}

fn plugin_delete(args: Value) -> Result<Value, String> {
    let plugin_name = s(&args, "name");
    let delete_all = b(&args, "all");
    let targets: Vec<String> = if delete_all {
        registry::get_installed().into_iter().map(|p| p.plugin_name).collect()
    } else if let Some(n) = plugin_name { vec![n.to_string()] }
    else { return Err("Specify a plugin name or all=true".into()); };
    let mut results = Vec::new();
    for name in targets {
        let Some(inst) = registry::get_installed_plugin(&name) else {
            results.push(json!({"name": name, "status": "not_installed"})); continue;
        };
        converter::set_scope(Scope::from_str(&inst.scope));
        let comps = registry::remove_installed(&name).map_err(err_str)?;
        for c in &comps { converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(err_str)?; }
        results.push(json!({"name": name, "status": "deleted", "components": comps.len()}));
    }
    Ok(Value::Array(results))
}

fn plugin_update(args: Value) -> Result<Value, String> {
    let plugin_name = s(&args, "name");
    let update_all = b(&args, "all");
    let only_types: Option<HashSet<String>> = vs(&args, "only_types").map(|v| v.into_iter().collect());
    let installed = registry::get_installed();
    let targets = if update_all { installed }
    else if let Some(n) = plugin_name { installed.into_iter().filter(|p| p.plugin_name == n).collect() }
    else { return Err("Specify a plugin name or all=true".into()); };
    if targets.is_empty() { return Err("No matching installed plugins".into()); }
    let mut updated_sources: HashSet<String> = HashSet::new();
    for p in &targets {
        if !updated_sources.contains(&p.source_name) {
            source::update_source(&p.source_name).map_err(err_str)?;
            updated_sources.insert(p.source_name.clone());
        }
    }
    let mut results = Vec::new();
    for p in targets {
        converter::set_scope(Scope::from_str(&p.scope));
        let plugins = source::parse_marketplace(&p.source_name).map_err(err_str)?;
        let Some(plugin) = plugins.into_iter().find(|pl| pl.name == p.plugin_name) else {
            results.push(json!({"name": p.plugin_name, "status": "not_found"})); continue;
        };
        let Some(lp) = plugin.local_path.clone() else { results.push(json!({"name": p.plugin_name, "status": "not_found"})); continue; };
        // Capture which components were disabled BEFORE remove/re-convert, so we can restore state
        let was_disabled: HashSet<(String, String)> = p.components.iter()
            .filter(|c| !converter::is_component_enabled(&c.component_type, &c.target_path, c.mcp_keys.as_deref()))
            .map(|c| (c.component_type.clone(), c.name.clone()))
            .collect();
        // Clean up old components, but skip MCP so 'disabled' flags survive (convert_mcp overwrites anyway)
        for c in &p.components {
            if c.component_type == "mcp" { continue; }
            converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(err_str)?;
        }
        let comps = scanner::scan_plugin(&lp, plugin.skill_filter.as_deref()).map_err(err_str)?;
        let comps: Vec<_> = comps.into_iter().filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type))).collect();
        let commit = source::list_sources().into_iter().find(|s| s.name == p.source_name).map(|s| s.commit).unwrap_or_default();
        let mut converted = Vec::new();
        for comp in comps {
            if converter::check_conflict(&comp.name, &comp.component_type, &p.plugin_name, &p.source_name).is_some() { continue; }
            let recs = converter::convert_component(&comp, &p.plugin_name, &p.source_name).map_err(err_str)?;
            converted.extend(recs);
        }
        // Restore disabled state for components that were previously disabled
        for rec in &converted {
            if was_disabled.contains(&(rec.component_type.clone(), rec.name.clone())) {
                let _ = converter::set_component_enabled(&rec.component_type, &rec.target_path, rec.mcp_keys.as_deref(), false);
            }
        }
        let n = converted.len();
        registry::add_installed(&p.plugin_name, &p.source_name, converted, &commit, &p.scope).map_err(err_str)?;
        results.push(json!({"name": p.plugin_name, "status": "updated", "components": n}));
    }
    Ok(Value::Array(results))
}

fn plugin_toggle(args: Value) -> Result<Value, String> {
    let plugin_name = s(&args, "name").ok_or("missing name")?;
    let component_name = s(&args, "component");
    let only_types: Option<HashSet<String>> = vs(&args, "only_types").map(|v| v.into_iter().collect());
    let enable = b(&args, "enable");
    let installed = registry::get_installed_plugin(plugin_name)
        .ok_or_else(|| format!("Plugin '{plugin_name}' is not installed"))?;
    converter::set_scope(Scope::from_str(&installed.scope));
    let mut touched = 0usize;
    for c in &installed.components {
        if let Some(n) = component_name { if c.name != n { continue; } }
        if let Some(f) = &only_types { if !f.contains(&c.component_type) { continue; } }
        converter::set_component_enabled(&c.component_type, &c.target_path, c.mcp_keys.as_deref(), enable).map_err(err_str)?;
        touched += 1;
    }
    Ok(json!({"touched": touched, "enable": enable}))
}


// ── config export/import ─────────────────────────────────────────────────

fn config_export() -> Result<Value, String> {
    let sources: Vec<Value> = source::list_sources().into_iter().map(|s| json!({"name": s.name, "url": s.url})).collect();
    let installed = registry::get_installed();
    let mut plugins = Vec::new();
    for p in installed {
        converter::set_scope(Scope::from_str(&p.scope));
        let comps: Vec<Value> = p.components.iter().map(|c| {
            let enabled = converter::is_component_enabled(&c.component_type, &c.target_path, c.mcp_keys.as_deref());
            json!({"type": c.component_type, "name": c.name, "enabled": enabled})
        }).collect();
        plugins.push(json!({
            "name": p.plugin_name,
            "source": p.source_name,
            "scope": p.scope,
            "components": comps,
        }));
    }
    Ok(json!({"version": 1, "sources": sources, "plugins": plugins}))
}

fn config_import(args: Value) -> Result<Value, String> {
    let config = if let Some(s) = args.get("config") { s.clone() } else { args.clone() };
    let ver = config.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if ver != 1 { return Err(format!("unsupported config version: {ver}")); }

    let sources_arr = config.get("sources").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let plugins_arr = config.get("plugins").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    println!("Importing {} sources, {} plugins", sources_arr.len(), plugins_arr.len());

    // 1. Add sources
    let mut source_results = Vec::new();
    for src in &sources_arr {
        let url = src.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let name = src.get("name").and_then(|v| v.as_str());
        if url.is_empty() { continue; }
        let display = name.unwrap_or(url);
        println!("  [source] fetching {}...", display);
        match source::add_source(url, name) {
            Ok(s) => {
                println!("  [source] ✓ {} @ {}", s.name, s.commit.chars().take(8).collect::<String>());
                source_results.push(json!({"name": s.name, "status": "ok"}));
            }
            Err(e) => {
                println!("  [source] ✗ {} — {}", display, e);
                source_results.push(json!({"name": display, "status": format!("{e}")}));
            }
        }
    }

    // 2. Install plugins + set enable/disable
    let mut plugin_results = Vec::new();
    for pl in &plugins_arr {
        let name = pl.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let scope = pl.get("scope").and_then(|v| v.as_str()).unwrap_or("global");
        if name.is_empty() { continue; }

        if registry::get_installed_plugin(name).is_none() {
            println!("  [plugin] installing {} ({})...", name, scope);
            let add_args = json!({"name": name, "scope": scope});
            match plugin_add(add_args) {
                Ok(v) => {
                    let n = v.as_array().and_then(|a| a.first()).and_then(|r| r.get("components")).and_then(|x| x.as_u64()).unwrap_or(0);
                    println!("  [plugin] ✓ {} ({} components)", name, n);
                }
                Err(e) => {
                    println!("  [plugin] ✗ {} — {}", name, e);
                }
            }
        } else {
            println!("  [plugin] {} already installed", name);
        }

        // Set enable/disable per component
        if let Some(comps) = pl.get("components").and_then(|v| v.as_array()) {
            let mut disabled_count = 0;
            for c in comps {
                let comp_name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let enabled = c.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                if !comp_name.is_empty() {
                    let toggle_args = json!({"name": name, "component": comp_name, "enable": enabled});
                    let _ = plugin_toggle(toggle_args);
                    if !enabled { disabled_count += 1; }
                }
            }
            if disabled_count > 0 {
                println!("  [plugin] {} — disabled {} component(s)", name, disabled_count);
            }
        }
        plugin_results.push(json!({"name": name, "status": "ok"}));
    }

    println!("Import complete");
    Ok(json!({"sources": source_results, "plugins": plugin_results}))
}

// ── update checks ────────────────────────────────────────────────────────

/// For each installed source, compare local commit with remote HEAD.
/// Returns: [{name, url, current, latest, has_update}]
fn source_check_updates() -> Result<Value, String> {
    let mut out = Vec::new();
    for s in source::list_sources() {
        let result = source::remote_head(&s.url);
        let entry = match result {
            Ok(latest) => {
                let has_update = !s.commit.is_empty() && !latest.starts_with(&s.commit) && !s.commit.starts_with(&latest);
                json!({
                    "name": s.name,
                    "url": s.url,
                    "current": s.commit,
                    "latest": latest,
                    "has_update": has_update,
                })
            }
            Err(e) => json!({
                "name": s.name,
                "url": s.url,
                "current": s.commit,
                "error": format!("{e}"),
            }),
        };
        out.push(entry);
    }
    Ok(Value::Array(out))
}

/// Check GitHub Releases for kiro-cc-plugins latest version.
/// args: { current: "0.3.4" }
/// Returns: { current, latest, has_update, download_url }
fn app_check_update(args: Value) -> Result<Value, String> {
    let current = args.get("current").and_then(|v| v.as_str()).unwrap_or("").trim_start_matches('v').to_string();
    let api_url = "https://api.github.com/repos/vokako/kiro-cc-plugins/releases/latest";

    let agent = ureq::AgentBuilder::new().user_agent("kiro-cc-plugins").timeout(std::time::Duration::from_secs(8)).build();
    let resp = agent.get(api_url).call().map_err(|e| format!("github api: {e}"))?;
    let body: Value = resp.into_json().map_err(|e| format!("parse json: {e}"))?;

    let tag = body.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    let latest = tag.trim_start_matches('v').to_string();
    let has_update = !latest.is_empty() && !current.is_empty() && version_newer(&latest, &current);

    // Find the macOS dmg asset (aarch64 for Apple Silicon, x64 for Intel)
    let assets = body.get("assets").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let target_arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let target_ext = if cfg!(target_os = "macos") { ".dmg" }
                     else if cfg!(target_os = "windows") { ".exe" }
                     else { ".tar.gz" };
    let asset_url = assets.iter()
        .find(|a| {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            name.ends_with(target_ext) && name.contains(target_arch)
        })
        .and_then(|a| a.get("browser_download_url").and_then(|v| v.as_str()))
        .map(String::from);

    let release_url = body.get("html_url").and_then(|v| v.as_str()).map(String::from);

    Ok(json!({
        "current": current,
        "latest": latest,
        "has_update": has_update,
        "download_url": asset_url,
        "release_url": release_url,
    }))
}

/// Return true if version `a` is newer than version `b`. Semver-ish comparison.
fn version_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.').filter_map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()).collect()
    };
    let aa = parse(a);
    let bb = parse(b);
    for i in 0..aa.len().max(bb.len()) {
        let x = aa.get(i).copied().unwrap_or(0);
        let y = bb.get(i).copied().unwrap_or(0);
        if x != y { return x > y; }
    }
    false
}
