//! Tauri backend — dispatches frontend commands directly to kiro_cc_core (no sidecar).

use kiro_cc_core::{
    converter, models::{self, Scope},
    registry,
    scanner,
    source::{self, MarketplacePlugin},
};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::Path;

#[tauri::command]
async fn call_api(request: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || dispatch(request))
        .await
        .map_err(|e| format!("join error: {e}"))?
}

fn dispatch(request: Value) -> Result<Value, String> {
    models::ensure_dirs().map_err(|e| format!("ensure_dirs: {e}"))?;
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
        other => Err(format!("Unknown command: {other}")),
    };
    Ok(match result {
        Ok(data) => json!({"success": true, "data": data}),
        Err(msg) => json!({"success": false, "error": msg}),
    })
}

fn as_str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}
fn as_bool(v: &Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}
fn as_vec_str(v: &Value, k: &str) -> Option<Vec<String>> {
    v.get(k).and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|i| i.as_str().map(String::from)).collect())
}
fn to_err<E: std::fmt::Display>(e: E) -> String { format!("{e}") }

// ── source handlers ──────────────────────────────────────────────────────

fn source_list() -> Result<Value, String> {
    let sources = source::list_sources();
    Ok(serde_json::to_value(sources).map_err(to_err)?)
}

fn source_add(args: Value) -> Result<Value, String> {
    let url = as_str(&args, "url").ok_or("missing url")?;
    let name = as_str(&args, "name");
    let s = source::add_source(url, name).map_err(to_err)?;
    let plugins = source::parse_marketplace(&s.name).map_err(to_err)?;
    Ok(json!({
        "source": {"name": s.name, "url": s.url, "commit": s.commit},
        "plugin_count": plugins.len(),
    }))
}

fn source_remove(args: Value) -> Result<Value, String> {
    let force = as_bool(&args, "force");
    let remove_all = as_bool(&args, "all");
    let targets: Vec<String> = if remove_all {
        if !force { return Err("--all requires force=true".into()); }
        source::list_sources().into_iter().map(|s| s.name).collect()
    } else if let Some(n) = as_str(&args, "name") {
        vec![n.to_string()]
    } else {
        return Err("Specify a source name or all=true".into());
    };

    let mut results = Vec::new();
    for sn in targets {
        let installed: Vec<_> = registry::get_installed().into_iter().filter(|p| p.source_name == sn).collect();
        if !installed.is_empty() && !force {
            let names: Vec<&str> = installed.iter().map(|p| p.plugin_name.as_str()).collect();
            return Err(format!(
                "Source '{sn}' has {} installed plugin(s): {}. Use force=true to uninstall them.",
                installed.len(), names.join(", ")
            ));
        }
        let mut uninstalled = Vec::new();
        for p in &installed {
            converter::set_scope(Scope::from_str(&p.scope));
            let components = registry::remove_installed(&p.plugin_name).map_err(to_err)?;
            for c in &components {
                converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(to_err)?;
            }
            uninstalled.push(p.plugin_name.clone());
        }
        source::remove_source(&sn).map_err(to_err)?;
        results.push(json!({"name": sn, "uninstalled_plugins": uninstalled}));
    }
    Ok(Value::Array(results))
}

fn source_update(args: Value) -> Result<Value, String> {
    let targets: Vec<String> = if let Some(n) = as_str(&args, "name") {
        vec![n.to_string()]
    } else {
        source::list_sources().into_iter().map(|s| s.name).collect()
    };
    let mut results = Vec::new();
    for sn in targets {
        let commit = source::update_source(&sn).map_err(to_err)?;
        results.push(json!({"name": sn, "commit": commit}));
    }
    Ok(Value::Array(results))
}

// ── plugin helpers ───────────────────────────────────────────────────────

fn find_plugin(name: &str) -> Option<(String, MarketplacePlugin)> {
    for s in source::list_sources() {
        if let Ok(plugins) = source::parse_marketplace(&s.name) {
            if let Some(p) = plugins.into_iter().find(|p| p.name == name) {
                return Some((s.name.clone(), p));
            }
        }
    }
    None
}

fn plugin_status(p: &MarketplacePlugin, installed_names: &HashSet<String>) -> &'static str {
    if installed_names.contains(&p.name) { "installed" }
    else if p.local_path.as_ref().is_some_and(|lp| lp.is_dir()) { "available" }
    else if p.source.is_object() { "fetchable" }
    else { "unavailable" }
}

// ── plugin handlers ──────────────────────────────────────────────────────

fn plugin_list(args: Value) -> Result<Value, String> {
    let source_name = as_str(&args, "source");
    let installed_only = as_bool(&args, "installed");
    let only_types: Option<HashSet<String>> = as_vec_str(&args, "only_types").map(|v| v.into_iter().collect());

    if installed_only {
        let mut items = registry::get_installed();
        if let Some(f) = &only_types {
            for p in &mut items {
                p.components.retain(|c| f.contains(&c.component_type));
            }
        }
        return Ok(serde_json::to_value(items).map_err(to_err)?);
    }

    let all_plugins: Vec<MarketplacePlugin> = if let Some(sn) = source_name {
        source::parse_marketplace(sn).map_err(to_err)?
    } else {
        let mut all = Vec::new();
        for s in source::list_sources() {
            if let Ok(p) = source::parse_marketplace(&s.name) { all.extend(p); }
        }
        all
    };

    let installed_names: HashSet<String> = registry::get_installed().into_iter().map(|p| p.plugin_name).collect();
    let mut result = Vec::new();
    for p in all_plugins {
        let status = plugin_status(&p, &installed_names);
        let mut entry = Map::new();
        entry.insert("name".into(), json!(p.name));
        entry.insert("description".into(), json!(p.description));
        entry.insert("category".into(), json!(p.category));
        entry.insert("status".into(), json!(status));

        if let Some(lp) = &p.local_path {
            if lp.is_dir() {
                let comps = scanner::scan_plugin(lp, p.skill_filter.as_deref()).map_err(to_err)?;
                let comp_list: Vec<Value> = comps.into_iter()
                    .filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type)))
                    .map(|c| json!({
                        "type": c.component_type,
                        "name": c.name,
                        "description": c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    }))
                    .collect();
                entry.insert("components".into(), Value::Array(comp_list));
            }
        }
        result.push(Value::Object(entry));
    }
    Ok(Value::Array(result))
}

fn plugin_detail(args: Value) -> Result<Value, String> {
    let name = as_str(&args, "name").ok_or("missing name")?;
    let (source_name, plugin) = find_plugin(name).ok_or_else(|| format!("Plugin '{name}' not found"))?;
    let installed = registry::get_installed_plugin(name);
    let mut components = Vec::new();
    if let Some(lp) = &plugin.local_path {
        if lp.is_dir() {
            let comps = scanner::scan_plugin(lp, plugin.skill_filter.as_deref()).map_err(to_err)?;
            components = comps.into_iter().map(|c| json!({
                "type": c.component_type,
                "name": c.name,
                "description": c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            })).collect();
        }
    }
    Ok(json!({
        "name": name,
        "description": plugin.description,
        "category": plugin.category,
        "source": source_name,
        "installed": installed.is_some(),
        "components": components,
    }))
}

fn plugin_add(args: Value) -> Result<Value, String> {
    let plugin_name = as_str(&args, "name");
    let mut source_name = as_str(&args, "source").map(String::from);
    let scope = as_str(&args, "scope").unwrap_or("global").to_string();
    let only_types: Option<HashSet<String>> = as_vec_str(&args, "only_types").map(|v| v.into_iter().collect());
    let install_all = as_bool(&args, "all");

    converter::set_scope(Scope::from_str(&scope));

    // Resolve source
    if let (Some(pn), None) = (plugin_name, source_name.as_deref()) {
        for s in source::list_sources() {
            if let Ok(plugins) = source::parse_marketplace(&s.name) {
                if plugins.iter().any(|p| p.name == pn) {
                    source_name = Some(s.name.clone());
                    break;
                }
            }
        }
    }
    let source_name = match source_name {
        Some(s) => s,
        None => {
            let sources = source::list_sources();
            if sources.len() == 1 { sources[0].name.clone() }
            else if plugin_name.is_some() { return Err(format!("Plugin '{}' not found in any source", plugin_name.unwrap())); }
            else { return Err("Multiple sources. Specify source.".into()); }
        }
    };

    let plugins = source::parse_marketplace(&source_name).map_err(to_err)?;
    let commit = source::list_sources().into_iter().find(|s| s.name == source_name).map(|s| s.commit).unwrap_or_default();

    let targets: Vec<MarketplacePlugin> = if install_all {
        plugins.into_iter().filter(|p| p.local_path.as_ref().is_some_and(|lp| lp.is_dir())).collect()
    } else if let Some(pn) = plugin_name {
        plugins.into_iter().filter(|p| p.name == pn).collect()
    } else {
        return Err("Specify a plugin name or all=true".into());
    };

    let mut results = Vec::new();
    for plugin in targets {
        let local_path = match &plugin.local_path {
            Some(lp) if lp.is_dir() => Some(lp.clone()),
            _ if plugin.source.is_object() => {
                match source::fetch_external_plugin(&plugin.name, &plugin.source).map_err(to_err)? {
                    Some(p) if p.is_dir() => Some(p),
                    _ => {
                        results.push(json!({"name": plugin.name, "status": "fetch_failed"}));
                        continue;
                    }
                }
            }
            _ => {
                results.push(json!({"name": plugin.name, "status": "unavailable"}));
                continue;
            }
        };
        let local_path = local_path.unwrap();

        let components = scanner::scan_plugin(&local_path, plugin.skill_filter.as_deref()).map_err(to_err)?;
        let components: Vec<_> = components.into_iter()
            .filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type)))
            .collect();

        let mut converted = Vec::new();
        let mut skipped = Vec::new();
        for comp in components {
            if let Some(conflict) = converter::check_conflict(&comp.name, &comp.component_type, &plugin.name, &source_name) {
                skipped.push(json!({"type": comp.component_type, "name": comp.name, "reason": conflict}));
                continue;
            }
            if let Some(rec) = converter::convert_component(&comp, &plugin.name, &source_name).map_err(to_err)? {
                converted.push(rec);
            }
        }
        let n = converted.len();
        if !converted.is_empty() {
            registry::add_installed(&plugin.name, &source_name, converted, &commit, &scope).map_err(to_err)?;
        }
        results.push(json!({"name": plugin.name, "status": "installed", "components": n, "skipped": skipped}));
    }
    Ok(Value::Array(results))
}

fn plugin_delete(args: Value) -> Result<Value, String> {
    let plugin_name = as_str(&args, "name");
    let delete_all = as_bool(&args, "all");
    let targets: Vec<String> = if delete_all {
        registry::get_installed().into_iter().map(|p| p.plugin_name).collect()
    } else if let Some(n) = plugin_name {
        vec![n.to_string()]
    } else {
        return Err("Specify a plugin name or all=true".into());
    };

    let mut results = Vec::new();
    for name in targets {
        let Some(installed) = registry::get_installed_plugin(&name) else {
            results.push(json!({"name": name, "status": "not_installed"}));
            continue;
        };
        converter::set_scope(Scope::from_str(&installed.scope));
        let components = registry::remove_installed(&name).map_err(to_err)?;
        for c in &components {
            converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(to_err)?;
        }
        results.push(json!({"name": name, "status": "deleted", "components": components.len()}));
    }
    Ok(Value::Array(results))
}

fn plugin_update(args: Value) -> Result<Value, String> {
    let plugin_name = as_str(&args, "name");
    let update_all = as_bool(&args, "all");
    let only_types: Option<HashSet<String>> = as_vec_str(&args, "only_types").map(|v| v.into_iter().collect());

    let installed = registry::get_installed();
    let targets = if update_all {
        installed
    } else if let Some(n) = plugin_name {
        installed.into_iter().filter(|p| p.plugin_name == n).collect()
    } else {
        return Err("Specify a plugin name or all=true".into());
    };
    if targets.is_empty() { return Err("No matching installed plugins".into()); }

    let mut updated_sources: HashSet<String> = HashSet::new();
    for p in &targets {
        if !updated_sources.contains(&p.source_name) {
            source::update_source(&p.source_name).map_err(to_err)?;
            updated_sources.insert(p.source_name.clone());
        }
    }

    let mut results = Vec::new();
    for p in targets {
        converter::set_scope(Scope::from_str(&p.scope));
        let plugins = source::parse_marketplace(&p.source_name).map_err(to_err)?;
        let Some(plugin) = plugins.into_iter().find(|pl| pl.name == p.plugin_name) else {
            results.push(json!({"name": p.plugin_name, "status": "not_found"}));
            continue;
        };
        let Some(local_path) = plugin.local_path.clone() else {
            results.push(json!({"name": p.plugin_name, "status": "not_found"}));
            continue;
        };
        for c in &p.components {
            converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(to_err)?;
        }
        let components = scanner::scan_plugin(&local_path, plugin.skill_filter.as_deref()).map_err(to_err)?;
        let components: Vec<_> = components.into_iter()
            .filter(|c| only_types.as_ref().is_none_or(|f| f.contains(&c.component_type)))
            .collect();
        let commit = source::list_sources().into_iter().find(|s| s.name == p.source_name).map(|s| s.commit).unwrap_or_default();
        let mut converted = Vec::new();
        for comp in components {
            if let Some(_) = converter::check_conflict(&comp.name, &comp.component_type, &p.plugin_name, &p.source_name) { continue; }
            if let Some(rec) = converter::convert_component(&comp, &p.plugin_name, &p.source_name).map_err(to_err)? {
                converted.push(rec);
            }
        }
        let n = converted.len();
        registry::add_installed(&p.plugin_name, &p.source_name, converted, &commit, &p.scope).map_err(to_err)?;
        results.push(json!({"name": p.plugin_name, "status": "updated", "components": n}));
    }
    Ok(Value::Array(results))
}

// ── entrypoint ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![call_api])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// silence unused in dispatch
#[allow(dead_code)]
fn _unused(_p: &Path) {}
