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
        "plugin.list" => plugin_list(args),
        "plugin.detail" => plugin_detail(args),
        "plugin.add" => plugin_add(args),
        "plugin.delete" => plugin_delete(args),
        "plugin.update" => plugin_update(args),
        "plugin.toggle" => plugin_toggle(args),
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
        }
    }
    Ok(json!({"name": name, "description": plugin.description, "category": plugin.category, "source": source_name, "installed": installed.is_some(), "components": components}))
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
            if let Some(rec) = converter::convert_component(&comp, &plugin.name, &source_name).map_err(err_str)? { converted.push(rec); }
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
        for c in &p.components { converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(err_str)?; }
        let comps = scanner::scan_plugin(&lp, plugin.skill_filter.as_deref()).map_err(err_str)?;
        let comps: Vec<_> = comps.into_iter().filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type))).collect();
        let commit = source::list_sources().into_iter().find(|s| s.name == p.source_name).map(|s| s.commit).unwrap_or_default();
        let mut converted = Vec::new();
        for comp in comps {
            if converter::check_conflict(&comp.name, &comp.component_type, &p.plugin_name, &p.source_name).is_some() { continue; }
            if let Some(rec) = converter::convert_component(&comp, &p.plugin_name, &p.source_name).map_err(err_str)? { converted.push(rec); }
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
