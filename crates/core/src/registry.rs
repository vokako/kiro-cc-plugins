//! Installation registry.

use crate::models::{load_json, registry_file, save_json, ComponentRecord, InstalledPlugin, Result};
use chrono::Utc;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct RegistryFile {
    #[serde(default)]
    installed: Vec<InstalledPlugin>,
}

fn load() -> RegistryFile {
    load_json(&registry_file()).unwrap_or_default()
}

fn save(data: &RegistryFile) -> Result<()> {
    save_json(&registry_file(), data)
}

pub fn add_installed(
    plugin_name: &str,
    source_name: &str,
    components: Vec<ComponentRecord>,
    commit: &str,
    scope: &str,
) -> Result<()> {
    let mut reg = load();
    reg.installed.retain(|p| p.plugin_name != plugin_name);
    reg.installed.push(InstalledPlugin {
        plugin_name: plugin_name.into(),
        source_name: source_name.into(),
        components,
        installed_at: Utc::now().to_rfc3339(),
        source_commit: commit.into(),
        scope: scope.into(),
    });
    save(&reg)
}

/// Remove plugin from registry, returning its components.
pub fn remove_installed(plugin_name: &str) -> Result<Vec<ComponentRecord>> {
    let mut reg = load();
    let mut removed = Vec::new();
    let mut remaining = Vec::new();
    for p in reg.installed {
        if p.plugin_name == plugin_name {
            removed = p.components;
        } else {
            remaining.push(p);
        }
    }
    reg.installed = remaining;
    save(&reg)?;
    Ok(removed)
}

pub fn get_installed() -> Vec<InstalledPlugin> {
    load().installed
}

pub fn get_installed_plugin(name: &str) -> Option<InstalledPlugin> {
    load().installed.into_iter().find(|p| p.plugin_name == name)
}
