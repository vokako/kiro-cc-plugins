//! Kiro CC Plugins CLI (Rust port of the Python CLI).

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_BORDERS_ONLY, Cell, Color, ContentArrangement, Table};
use console::style;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use kiro_cc_core::{
    converter, models,
    registry,
    scanner,
    source::{self, MarketplacePlugin},
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

fn make_table() -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_BORDERS_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic);
    if let Some((w, _)) = term_size::dimensions() {
        t.set_width(w as u16);
    } else {
        t.set_width(100);
    }
    t
}

#[derive(Parser)]
#[command(name = "kiro-cc-plugins", version, about = "Convert Claude Code plugins to Kiro format")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Manage plugin sources (git repos)
    Source {
        #[command(subcommand)]
        cmd: SourceCmd,
    },
    /// List plugins
    List {
        plugin: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        installed: bool,
        #[arg(long)]
        only: Option<String>,
        #[arg(long)]
        skills: bool,
        #[arg(long)]
        agents: bool,
    },
    /// Install plugins
    Add {
        plugin: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long = "all")]
        install_all: bool,
        #[arg(long)]
        only: Option<String>,
        #[arg(long)]
        git: Option<String>,
        #[arg(long, default_value = "global")]
        scope: String,
    },
    /// Delete installed plugins
    #[command(alias = "remove")]
    Delete {
        plugin: Option<String>,
        #[arg(long = "all")]
        delete_all: bool,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Update installed plugins (re-sync + re-convert)
    Update {
        plugin: Option<String>,
        #[arg(long = "all")]
        update_all: bool,
        #[arg(long)]
        only: Option<String>,
    },
    /// Enable components of an installed plugin (or a specific component)
    Enable {
        /// Plugin name (applies to all its components unless --component given)
        plugin: String,
        /// Specific component name (e.g. skill name)
        #[arg(long)]
        component: Option<String>,
        /// Only enable components of given types (skill,agent,command,mcp)
        #[arg(long)]
        only: Option<String>,
    },
    /// Disable components of an installed plugin (or a specific component)
    Disable {
        plugin: String,
        #[arg(long)]
        component: Option<String>,
        #[arg(long)]
        only: Option<String>,
    },
    /// Internal JSON API for the desktop app (hidden)
    #[command(hide = true)]
    Api {
        /// JSON request string: {"command":"...","args":{...}}
        request: String,
    },
}

#[derive(Subcommand)]
enum SourceCmd {
    /// Clone a marketplace or plugin repo
    Add {
        url: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// List registered sources
    List,
    /// Remove a source (or all with --all)
    #[command(alias = "delete")]
    Remove {
        name: Option<String>,
        #[arg(short, long)]
        force: bool,
        #[arg(long = "all")]
        remove_all: bool,
    },
    /// Git pull a source (or all)
    Update { name: Option<String> },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {e:#}", style("Error").red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    models::ensure_dirs().context("ensure_dirs")?;
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Source { cmd } => match cmd {
            SourceCmd::Add { url, name } => source_add(&url, name.as_deref()),
            SourceCmd::List => source_list(),
            SourceCmd::Remove { name, force, remove_all } => source_remove(name.as_deref(), force, remove_all),
            SourceCmd::Update { name } => source_update(name.as_deref()),
        },
        Cmd::List { plugin, source, installed, only, skills, agents } => {
            list_cmd(plugin.as_deref(), source.as_deref(), installed, only.as_deref(), skills, agents)
        }
        Cmd::Add { plugin, source, install_all, only, git, scope } => {
            add_cmd(plugin.as_deref(), source.as_deref(), install_all, only.as_deref(), git.as_deref(), &scope)
        }
        Cmd::Delete { plugin, delete_all, yes } => delete_cmd(plugin.as_deref(), delete_all, yes),
        Cmd::Update { plugin, update_all, only } => update_cmd(plugin.as_deref(), update_all, only.as_deref()),
        Cmd::Enable { plugin, component, only } => toggle_cmd(&plugin, component.as_deref(), only.as_deref(), true),
        Cmd::Disable { plugin, component, only } => toggle_cmd(&plugin, component.as_deref(), only.as_deref(), false),
        Cmd::Api { request } => api_cmd(&request),
    }
}

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb.set_style(ProgressStyle::with_template("{spinner} {msg}").unwrap());
    pb
}

fn ok(msg: impl std::fmt::Display) {
    println!("{} {msg}", style("✓").green());
}

fn warn(msg: impl std::fmt::Display) {
    println!("{} {msg}", style("⚠").yellow());
}

fn dim(msg: impl std::fmt::Display) {
    println!("{}", style(msg).dim());
}

fn parse_only(only: Option<&str>) -> Option<HashSet<String>> {
    only.map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
}

// ── source commands ──────────────────────────────────────────────────────

fn source_add(url: &str, name: Option<&str>) -> Result<()> {
    let pb = spinner(&format!("Cloning {url}..."));
    let s = source::add_source(url, name).map_err(|e| anyhow!("{e}"))?;
    pb.finish_and_clear();
    ok(format!(
        "Source {} added ({})",
        style(&s.name).bold(),
        s.commit.chars().take(8).collect::<String>()
    ));
    let plugins = source::parse_marketplace(&s.name).map_err(|e| anyhow!("{e}"))?;
    let local_count = plugins.iter().filter(|p| p.local_path.as_ref().is_some_and(|p| p.is_dir())).count();
    println!("  {} plugins found, {local_count} available locally", plugins.len());
    Ok(())
}

fn source_list() -> Result<()> {
    let sources = source::list_sources();
    if sources.is_empty() {
        dim("No sources. Run: kiro-cc-plugins source add <url>");
        return Ok(());
    }
    let mut t = make_table();
    t.set_header(vec![Cell::new("Name").fg(Color::DarkGrey), Cell::new("URL").fg(Color::DarkGrey), Cell::new("Commit").fg(Color::DarkGrey)]);
    for s in sources {
        t.add_row(vec![
            Cell::new(s.name).fg(Color::White),
            Cell::new(s.url),
            Cell::new(s.commit.chars().take(8).collect::<String>()),
        ]);
    }
    println!("{t}");
    Ok(())
}

fn source_remove(name: Option<&str>, force: bool, remove_all: bool) -> Result<()> {
    let targets: Vec<String> = if remove_all {
        if !force {
            return Err(anyhow!("--all requires --force"));
        }
        source::list_sources().into_iter().map(|s| s.name).collect()
    } else if let Some(n) = name {
        vec![n.to_string()]
    } else {
        return Err(anyhow!("Specify a source name or --all"));
    };

    for sn in targets {
        let installed: Vec<_> = registry::get_installed().into_iter().filter(|p| p.source_name == sn).collect();
        if !installed.is_empty() && !force {
            let names: Vec<&str> = installed.iter().map(|p| p.plugin_name.as_str()).collect();
            return Err(anyhow!(
                "Source '{sn}' has {} installed plugin(s): {}.\nUse --force to uninstall them and remove the source.",
                installed.len(),
                names.join(", ")
            ));
        }
        for p in &installed {
            converter::set_scope(models::Scope::from_str(&p.scope));
            let components = registry::remove_installed(&p.plugin_name).map_err(|e| anyhow!("{e}"))?;
            for c in &components {
                converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(|e| anyhow!("{e}"))?;
            }
            warn(format!("Uninstalled {} ({} components)", p.plugin_name, components.len()));
        }
        source::remove_source(&sn).map_err(|e| anyhow!("{e}"))?;
        ok(format!("Source {} removed", style(&sn).bold()));
    }
    Ok(())
}

fn source_update(name: Option<&str>) -> Result<()> {
    let targets: Vec<String> = if let Some(n) = name {
        vec![n.to_string()]
    } else {
        source::list_sources().into_iter().map(|s| s.name).collect()
    };
    for sn in targets {
        let pb = spinner(&format!("Updating {sn}..."));
        let commit = source::update_source(&sn).map_err(|e| anyhow!("{e}"))?;
        pb.finish_and_clear();
        ok(format!("{sn} updated ({})", commit.chars().take(8).collect::<String>()));
    }
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────

fn find_plugin_source(name: &str) -> Result<Option<(String, Vec<MarketplacePlugin>)>> {
    let mut matches = Vec::new();
    for s in source::list_sources() {
        let Ok(plugins) = source::parse_marketplace(&s.name) else { continue };
        if plugins.iter().any(|p| p.name == name) {
            matches.push((s.name.clone(), plugins));
        }
    }
    if matches.len() == 1 {
        Ok(Some(matches.into_iter().next().unwrap()))
    } else if matches.len() > 1 {
        let names: Vec<&str> = matches.iter().map(|m| m.0.as_str()).collect();
        Err(anyhow!("'{name}' found in multiple sources: {}. Specify --source.", names.join(", ")))
    } else {
        Ok(None)
    }
}

fn resolve_source(source_name: Option<&str>) -> Result<String> {
    if let Some(s) = source_name {
        return Ok(s.to_string());
    }
    let sources = source::list_sources();
    if sources.len() == 1 {
        Ok(sources[0].name.clone())
    } else {
        Err(anyhow!("Multiple sources found. Specify --source <name>."))
    }
}

fn get_commit(source_name: &str) -> String {
    source::list_sources().into_iter().find(|s| s.name == source_name).map(|s| s.commit).unwrap_or_default()
}

// ── list command ─────────────────────────────────────────────────────────

fn list_cmd(
    plugin_name: Option<&str>,
    source_name: Option<&str>,
    installed: bool,
    only: Option<&str>,
    list_skills: bool,
    list_agents: bool,
) -> Result<()> {
    let type_filter = parse_only(only);

    if list_skills || list_agents {
        let comp_type = if list_skills { "skill" } else { "agent" };
        return list_all_components(comp_type, source_name);
    }
    if installed {
        return list_installed(&type_filter);
    }

    let (sn, all_plugins): (Option<String>, Vec<MarketplacePlugin>) = if let Some(s) = source_name {
        (Some(s.to_string()), source::parse_marketplace(s).map_err(|e| anyhow!("{e}"))?)
    } else if let Some(pn) = plugin_name {
        if let Some((sn, plugins)) = find_plugin_source(pn)? {
            (Some(sn), plugins)
        } else {
            let sn = resolve_source(None)?;
            let plugins = source::parse_marketplace(&sn).map_err(|e| anyhow!("{e}"))?;
            (Some(sn), plugins)
        }
    } else {
        // merge all
        let mut all = Vec::new();
        for s in source::list_sources() {
            if let Ok(p) = source::parse_marketplace(&s.name) {
                all.extend(p);
            }
        }
        (None, all)
    };

    if let Some(pn) = plugin_name {
        show_plugin_detail(&sn.unwrap_or_default(), &all_plugins, pn, &type_filter)
    } else {
        list_plugins(&all_plugins, &type_filter)
    }
}

fn list_all_components(comp_type: &str, source_name: Option<&str>) -> Result<()> {
    let installed_targets: HashSet<String> = registry::get_installed()
        .iter()
        .flat_map(|p| p.components.iter().map(|c| c.name.clone()))
        .collect();

    let sources: Vec<String> = match source_name {
        Some(s) => vec![s.to_string()],
        None => source::list_sources().into_iter().map(|s| s.name).collect(),
    };

    let mut t = make_table();
    t.set_header(vec!["Name", "Plugin", "Source", "Description", "Status"]);
    let mut total = 0usize;

    for sn in sources {
        let Ok(plugins) = source::parse_marketplace(&sn) else { continue };
        for plugin in plugins {
            let Some(lp) = &plugin.local_path else { continue };
            if !lp.is_dir() {
                continue;
            }
            let Ok(comps) = scanner::scan_plugin(lp, plugin.skill_filter.as_deref()) else {
                continue;
            };
            for c in comps {
                if c.component_type != comp_type {
                    continue;
                }
                let status = if installed_targets.contains(&c.name) {
                    Cell::new("✓").fg(Color::Green)
                } else {
                    Cell::new("○").fg(Color::DarkGrey)
                };
                let desc = c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                t.add_row(vec![Cell::new(c.name), Cell::new(&plugin.name), Cell::new(&sn), Cell::new(desc), status]);
                total += 1;
            }
        }
    }
    println!("{t}");
    dim(format!("\n{total} {comp_type}s found across local plugins."));
    Ok(())
}

fn list_installed(type_filter: &Option<HashSet<String>>) -> Result<()> {
    let items = registry::get_installed();
    if items.is_empty() {
        dim("No plugins installed.");
        return Ok(());
    }
    let mut t = make_table();
    t.set_header(vec!["Plugin", "Source", "Components", "Installed"]);
    for p in items {
        let comps: Vec<_> = p.components.iter().filter(|c| type_filter.as_ref().is_none_or(|f| f.contains(&c.component_type))).collect();
        let summary = comps.iter().map(|c| format!("{}:{}", c.component_type, c.name)).collect::<Vec<_>>().join(", ");
        let date = p.installed_at.chars().take(10).collect::<String>();
        t.add_row(vec![p.plugin_name, p.source_name, summary, date]);
    }
    println!("{t}");
    Ok(())
}

fn plugin_status(p: &MarketplacePlugin, installed_names: &HashSet<String>) -> (Cell, String) {
    if installed_names.contains(&p.name) {
        (Cell::new("✓").fg(Color::Green), "installed".into())
    } else if p.local_path.as_ref().is_some_and(|lp| lp.is_dir()) {
        (Cell::new("○").fg(Color::Green), "available".into())
    } else if p.source.is_object() {
        (Cell::new("○").fg(Color::Yellow), "fetchable".into())
    } else {
        (Cell::new("✗").fg(Color::Red), "unavailable".into())
    }
}

fn list_plugins(plugins: &[MarketplacePlugin], _type_filter: &Option<HashSet<String>>) -> Result<()> {
    let installed_names: HashSet<String> = registry::get_installed().into_iter().map(|p| p.plugin_name).collect();
    let mut t = make_table();
    t.set_header(vec!["Name", "Description", "Category", "Status"]);
    let mut n_installed = 0;
    let mut n_local = 0;
    let mut n_fetch = 0;
    for p in plugins {
        let (icon, kind) = plugin_status(p, &installed_names);
        match kind.as_str() {
            "installed" => n_installed += 1,
            "available" => n_local += 1,
            "fetchable" => n_fetch += 1,
            _ => {}
        }
        t.add_row(vec![Cell::new(&p.name), Cell::new(&p.description), Cell::new(&p.category), icon]);
    }
    println!("{t}");
    dim(format!(
        "\n{} plugins: {} installed, {} available, {} fetchable",
        plugins.len(),
        style(n_installed).green(),
        style(n_local).green(),
        style(n_fetch).yellow(),
    ));
    Ok(())
}

fn show_plugin_detail(
    _source_name: &str,
    plugins: &[MarketplacePlugin],
    name: &str,
    type_filter: &Option<HashSet<String>>,
) -> Result<()> {
    let plugin = plugins.iter().find(|p| p.name == name).ok_or_else(|| anyhow!("Plugin '{name}' not found"))?;
    let installed = registry::get_installed_plugin(name);
    let status = if installed.is_some() { style("(installed)").green().to_string() } else { String::new() };
    println!("\n{} {status}", style(&plugin.name).bold());
    println!("  {}", plugin.description);

    let Some(lp) = &plugin.local_path else {
        if plugin.source.is_object() {
            dim("\n  External — run 'kiro-cc-plugins add <name>' to fetch and install.");
        } else {
            warn("Not available locally");
        }
        return Ok(());
    };
    if !lp.is_dir() {
        dim("  Not available locally");
        return Ok(());
    }
    let components = scanner::scan_plugin(lp, plugin.skill_filter.as_deref()).map_err(|e| anyhow!("{e}"))?;
    let components: Vec<_> = components
        .into_iter()
        .filter(|c| type_filter.as_ref().is_none_or(|f| f.contains(&c.component_type)))
        .collect();

    if components.is_empty() {
        dim("  No convertible components found.");
        return Ok(());
    }
    let mut t = make_table();
    t.set_header(vec!["Type", "Name", "Description"]);
    for c in &components {
        let desc = c.frontmatter.get("description").and_then(|v| v.as_str()).unwrap_or("");
        t.add_row(vec![c.component_type.clone(), c.name.clone(), desc.to_string()]);
    }
    println!("{t}");

    if installed.is_some() {
        let agents: Vec<&_> = components.iter().filter(|c| c.component_type == "agent" || c.component_type == "command").collect();
        if !agents.is_empty() {
            dim("\nRun with:");
            for a in agents {
                println!("  kiro-cli chat --agent {}", a.name);
            }
        }
    }
    Ok(())
}

// ── add command ──────────────────────────────────────────────────────────

fn add_cmd(
    plugin_name: Option<&str>,
    source_name: Option<&str>,
    install_all: bool,
    only: Option<&str>,
    git_url: Option<&str>,
    scope: &str,
) -> Result<()> {
    let scope_enum = models::Scope::from_str(scope);
    converter::set_scope(scope_enum);
    let type_filter = parse_only(only);

    let (source_name_resolved, plugins, install_all) = if let Some(url) = git_url {
        let s = source::add_source(url, None).map_err(|e| anyhow!("{e}"))?;
        let plugins = source::parse_marketplace(&s.name).map_err(|e| anyhow!("{e}"))?;
        (s.name, plugins, true)
    } else if let Some(pn) = plugin_name {
        if let Some((sn, plugins)) = find_plugin_source(pn)? {
            (sn, plugins, install_all)
        } else {
            let sn = resolve_source(source_name)?;
            let plugins = source::parse_marketplace(&sn).map_err(|e| anyhow!("{e}"))?;
            (sn, plugins, install_all)
        }
    } else {
        let sn = resolve_source(source_name)?;
        let plugins = source::parse_marketplace(&sn).map_err(|e| anyhow!("{e}"))?;
        (sn, plugins, install_all)
    };

    let commit = get_commit(&source_name_resolved);

    let targets: Vec<&MarketplacePlugin> = if install_all {
        plugins.iter().filter(|p| p.local_path.as_ref().is_some_and(|lp| lp.is_dir())).collect()
    } else if let Some(pn) = plugin_name {
        plugins.iter().filter(|p| p.name == pn).collect()
    } else {
        return Err(anyhow!("Specify a plugin name or --all"));
    };

    if targets.is_empty() {
        return Err(anyhow!("No matching plugins found"));
    }

    let mut total_components = 0usize;
    let mut converted_names: Vec<String> = Vec::new();

    for plugin in targets {
        let local_path: Option<PathBuf> = match &plugin.local_path {
            Some(lp) if lp.is_dir() => Some(lp.clone()),
            _ if plugin.source.is_object() => {
                let pb = spinner(&format!("Fetching {}...", plugin.name));
                let fetched = source::fetch_external_plugin(&plugin.name, &plugin.source).map_err(|e| anyhow!("{e}"))?;
                pb.finish_and_clear();
                match fetched {
                    Some(p) if p.is_dir() => Some(p),
                    _ => {
                        warn(format!("Failed to fetch {}", plugin.name));
                        continue;
                    }
                }
            }
            _ => {
                warn(format!("Skipping {} (not available)", plugin.name));
                continue;
            }
        };
        let local_path = local_path.unwrap();

        let components = scanner::scan_plugin(&local_path, plugin.skill_filter.as_deref()).map_err(|e| anyhow!("{e}"))?;
        let components: Vec<_> = components
            .into_iter()
            .filter(|c| type_filter.as_ref().is_none_or(|f| f.contains(&c.component_type)))
            .collect();

        if components.is_empty() {
            continue;
        }

        let mut converted = Vec::new();
        for comp in components {
            if let Some(conflict) = converter::check_conflict(&comp.name, &comp.component_type, &plugin.name, &source_name_resolved) {
                warn(format!("Skipping {}:{} — {conflict}", comp.component_type, comp.name));
                continue;
            }
            if let Some(rec) = converter::convert_component(&comp, &plugin.name, &source_name_resolved).map_err(|e| anyhow!("{e}"))? {
                if rec.component_type == "agent" || rec.component_type == "command" {
                    converted_names.push(rec.name.clone());
                }
                converted.push(rec);
                total_components += 1;
            }
        }
        if !converted.is_empty() {
            registry::add_installed(&plugin.name, &source_name_resolved, converted.clone(), &commit, scope).map_err(|e| anyhow!("{e}"))?;
            let types: std::collections::BTreeSet<&str> = converted.iter().map(|c| c.component_type.as_str()).collect();
            ok(format!("{} → {} components ({})", plugin.name, converted.len(), types.into_iter().collect::<Vec<_>>().join(", ")));
        }
    }

    let target_dir = if scope_enum == models::Scope::Global { "~/.kiro/" } else { ".kiro/" };
    println!("\n{} {total_components} components installed to {target_dir}", style("Done.").green());

    if !converted_names.is_empty() {
        dim("\nRun with:");
        for n in converted_names {
            println!("  kiro-cli chat --agent {n}");
        }
    }
    Ok(())
}

// ── delete command ───────────────────────────────────────────────────────

fn delete_cmd(plugin_name: Option<&str>, delete_all: bool, yes: bool) -> Result<()> {
    let targets: Vec<String> = if delete_all {
        registry::get_installed().into_iter().map(|p| p.plugin_name).collect()
    } else if let Some(n) = plugin_name {
        vec![n.to_string()]
    } else {
        return Err(anyhow!("Specify a plugin name or --all"));
    };

    for n in &targets {
        if registry::get_installed_plugin(n).is_none() {
            let installed_names: Vec<String> = registry::get_installed()
                .iter()
                .map(|p| format!("  {} (source: {})", p.plugin_name, p.source_name))
                .collect();
            let mut msg = format!("Plugin '{n}' is not installed.");
            if !installed_names.is_empty() {
                msg.push_str("\nInstalled plugins:\n");
                msg.push_str(&installed_names.join("\n"));
            }
            return Err(anyhow!(msg));
        }
    }

    if !yes {
        let details: Vec<String> = targets
            .iter()
            .map(|n| {
                let p = registry::get_installed_plugin(n).unwrap();
                format!("{} (source: {})", n, p.source_name)
            })
            .collect();
        if !Confirm::new().with_prompt(format!("Delete {}?", details.join(", "))).interact()? {
            return Err(anyhow!("Aborted"));
        }
    }

    for n in targets {
        let installed = registry::get_installed_plugin(&n).unwrap();
        converter::set_scope(models::Scope::from_str(&installed.scope));
        let components = registry::remove_installed(&n).map_err(|e| anyhow!("{e}"))?;
        for c in &components {
            converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(|e| anyhow!("{e}"))?;
        }
        ok(format!("{} (source: {}) removed ({} components)", n, installed.source_name, components.len()));
    }
    Ok(())
}

// ── update command ───────────────────────────────────────────────────────

fn update_cmd(plugin_name: Option<&str>, update_all: bool, only: Option<&str>) -> Result<()> {
    let type_filter = parse_only(only);
    let installed = registry::get_installed();
    let targets: Vec<_> = if update_all {
        installed
    } else if let Some(n) = plugin_name {
        installed.into_iter().filter(|p| p.plugin_name == n).collect()
    } else {
        return Err(anyhow!("Specify a plugin name or --all"));
    };

    if targets.is_empty() {
        return Err(anyhow!("No matching installed plugins"));
    }

    let mut updated_sources: HashSet<String> = HashSet::new();
    for p in &targets {
        if !updated_sources.contains(&p.source_name) {
            let pb = spinner(&format!("Updating source {}...", p.source_name));
            source::update_source(&p.source_name).map_err(|e| anyhow!("{e}"))?;
            pb.finish_and_clear();
            updated_sources.insert(p.source_name.clone());
        }
    }

    for p in targets {
        let scope_enum = models::Scope::from_str(&p.scope);
        converter::set_scope(scope_enum);
        let plugins = source::parse_marketplace(&p.source_name).map_err(|e| anyhow!("{e}"))?;
        let Some(plugin) = plugins.into_iter().find(|pl| pl.name == p.plugin_name) else {
            warn(format!("{} not found in source", p.plugin_name));
            continue;
        };
        let Some(local_path) = plugin.local_path.clone() else {
            warn(format!("{} has no local path", p.plugin_name));
            continue;
        };
        for c in &p.components {
            converter::remove_converted(&c.target_path, c.mcp_keys.as_deref()).map_err(|e| anyhow!("{e}"))?;
        }
        let components = scanner::scan_plugin(&local_path, plugin.skill_filter.as_deref()).map_err(|e| anyhow!("{e}"))?;
        let components: Vec<_> = components
            .into_iter()
            .filter(|c| type_filter.as_ref().is_none_or(|f| f.contains(&c.component_type)))
            .collect();
        let commit = get_commit(&p.source_name);
        let mut converted = Vec::new();
        for comp in components {
            if let Some(_conflict) = converter::check_conflict(&comp.name, &comp.component_type, &p.plugin_name, &p.source_name) {
                continue;
            }
            if let Some(rec) = converter::convert_component(&comp, &p.plugin_name, &p.source_name).map_err(|e| anyhow!("{e}"))? {
                converted.push(rec);
            }
        }
        registry::add_installed(&p.plugin_name, &p.source_name, converted.clone(), &commit, &p.scope).map_err(|e| anyhow!("{e}"))?;
        ok(format!("{} updated ({} components)", p.plugin_name, converted.len()));
    }
    Ok(())
}


// ── enable/disable ───────────────────────────────────────────────────────

fn toggle_cmd(plugin_name: &str, component_name: Option<&str>, only: Option<&str>, enable: bool) -> Result<()> {
    let type_filter = parse_only(only);
    let installed = registry::get_installed_plugin(plugin_name)
        .ok_or_else(|| anyhow!("Plugin '{plugin_name}' is not installed"))?;
    converter::set_scope(models::Scope::from_str(&installed.scope));

    let mut touched = 0usize;
    for c in &installed.components {
        if let Some(name) = component_name {
            if c.name != name { continue; }
        }
        if let Some(f) = &type_filter {
            if !f.contains(&c.component_type) { continue; }
        }
        converter::set_component_enabled(&c.component_type, &c.target_path, c.mcp_keys.as_deref(), enable)
            .map_err(|e| anyhow!("{e}"))?;
        let verb = if enable { "enabled" } else { "disabled" };
        ok(format!("{} {}:{} {verb}", plugin_name, c.component_type, c.name));
        touched += 1;
    }
    if touched == 0 {
        warn(format!("No matching components in {plugin_name}"));
    }
    Ok(())
}


// ── api (JSON mode for GUI) ──────────────────────────────────────────────

fn api_cmd(request_json: &str) -> Result<()> {
    let request: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|e| anyhow!("invalid JSON: {e}"))?;
    let result = kiro_cc_core::api::dispatch(&request);
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
