"""CLI entry point."""

from __future__ import annotations

from pathlib import Path

import click
from rich.console import Console
from rich.table import Table

from . import source as src_mod
from . import scanner, converter, registry
from .models import ensure_dirs

console = Console()


# ── helpers ──────────────────────────────────────────────────────────────

def _default_source() -> str | None:
    sources = src_mod.list_sources()
    if len(sources) == 1:
        return sources[0]["name"]
    return None


def _resolve_source(source_name: str | None) -> str:
    if source_name:
        return source_name
    default = _default_source()
    if default:
        return default
    raise click.ClickException("Multiple sources found. Specify --source <name>.")


def _find_plugin_source(plugin_name: str) -> tuple[str, list[dict]] | None:
    """Search all sources for a plugin by name. Returns (source_name, plugins) or None."""
    matches = []
    for s in src_mod.list_sources():
        try:
            plugins = src_mod.parse_marketplace(s["name"])
        except FileNotFoundError:
            continue
        if any(p["name"] == plugin_name for p in plugins):
            matches.append((s["name"], plugins))
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        names = ", ".join(m[0] for m in matches)
        raise click.ClickException(f"'{plugin_name}' found in multiple sources: {names}. Specify --source <name>.")
    return None


def _get_commit(source_name: str) -> str:
    for s in src_mod.list_sources():
        if s["name"] == source_name:
            return s.get("commit", "")
    return ""


# ── source commands ──────────────────────────────────────────────────────

@click.group()
def cli():
    """Convert Claude Code plugins to Kiro format."""
    ensure_dirs()


@cli.group("source")
def source_group():
    """Manage plugin sources (git repos)."""


@source_group.command("add")
@click.argument("url")
@click.option("--name", default=None, help="Override source name")
def source_add(url: str, name: str | None):
    """Clone a marketplace or plugin repo."""
    with console.status(f"Cloning {url}..."):
        s = src_mod.add_source(url, name)
    console.print(f"[green]✓[/] Source [bold]{s.name}[/] added ({s.commit[:8]})")

    plugins = src_mod.parse_marketplace(s.name)
    local_count = sum(1 for p in plugins if p.get("_local_path"))
    console.print(f"  {len(plugins)} plugins found, {local_count} available locally")


@source_group.command("list")
def source_list():
    """List registered sources."""
    sources = src_mod.list_sources()
    if not sources:
        console.print("[dim]No sources. Run: kiro-cc source add <url>[/]")
        return
    t = Table(title="Sources")
    t.add_column("Name", style="bold")
    t.add_column("URL")
    t.add_column("Commit")
    for s in sources:
        t.add_row(s["name"], s["url"], s.get("commit", "")[:8])
    console.print(t)


@source_group.command("remove")
@click.argument("name")
def source_remove(name: str):
    """Remove a source."""
    src_mod.remove_source(name)
    console.print(f"[green]✓[/] Source [bold]{name}[/] removed")


@source_group.command("update")
@click.argument("name", required=False)
def source_update(name: str | None):
    """Git pull a source."""
    if name is None:
        for s in src_mod.list_sources():
            commit = src_mod.update_source(s["name"])
            console.print(f"[green]✓[/] {s['name']} updated ({commit[:8]})")
    else:
        commit = src_mod.update_source(name)
        console.print(f"[green]✓[/] {name} updated ({commit[:8]})")


# ── list ─────────────────────────────────────────────────────────────────

@cli.command("list")
@click.argument("plugin_name", required=False)
@click.option("--source", "source_name", default=None, help="Source name")
@click.option("--installed", is_flag=True, help="Show installed plugins only")
@click.option("--only", "only_types", default=None, help="Filter by type: skill,agent,command,mcp")
@click.option("--skills", is_flag=True, help="List all skills across all sources")
@click.option("--agents", is_flag=True, help="List all agents across all sources")
def list_cmd(plugin_name: str | None, source_name: str | None, installed: bool, only_types: str | None, skills: bool, agents: bool):
    """List available or installed plugins."""
    type_filter = set(only_types.split(",")) if only_types else None

    if skills or agents:
        _list_all_components("skill" if skills else "agent", source_name)
        return

    if installed:
        _list_installed(type_filter)
        return

    # Collect plugins from specified source or all sources
    if source_name:
        all_plugins = src_mod.parse_marketplace(source_name)
    elif plugin_name:
        # Auto-find which source has this plugin
        found = _find_plugin_source(plugin_name)
        if found:
            source_name, all_plugins = found
        else:
            source_name = _resolve_source(None)
            all_plugins = src_mod.parse_marketplace(source_name)
    else:
        # Merge all sources
        all_plugins = []
        for s in src_mod.list_sources():
            try:
                all_plugins.extend(src_mod.parse_marketplace(s["name"]))
            except FileNotFoundError:
                continue

    if plugin_name:
        _show_plugin_detail(source_name or "", all_plugins, plugin_name, type_filter)
    else:
        _list_plugins(all_plugins, type_filter)


def _list_all_components(comp_type: str, source_name: str | None):
    """List all components of a given type across sources."""
    sources = [{"name": source_name}] if source_name else src_mod.list_sources()
    installed_targets = set()
    for p in registry.get_installed():
        for c in p.get("components", []):
            installed_targets.add(c.get("name", ""))

    label = comp_type.capitalize() + "s"
    t = Table(title=f"All {label}")
    t.add_column("Name", style="bold")
    t.add_column("Plugin")
    t.add_column("Source")
    t.add_column("Description", max_width=45)
    t.add_column("Status", justify="center")

    total = 0
    for src in sources:
        sn = src["name"]
        try:
            plugins = src_mod.parse_marketplace(sn)
        except FileNotFoundError:
            continue
        for plugin in plugins:
            lp = plugin.get("_local_path")
            if not lp or not Path(lp).is_dir():
                continue
            comps = scanner.scan_plugin(Path(lp), plugin.get("_skill_filter"))
            for c in comps:
                if c.type != comp_type:
                    continue
                status = "[green]✓[/]" if c.name in installed_targets else "[dim]○[/]"
                desc = c.frontmatter.get("description", "")[:45]
                t.add_row(c.name, plugin["name"], sn, desc, status)
                total += 1

    console.print(t)
    console.print(f"\n[dim]{total} {comp_type}s found across local plugins.[/]")


def _list_installed(type_filter: set | None):
    items = registry.get_installed()
    if not items:
        console.print("[dim]No plugins installed.[/]")
        return
    t = Table(title="Installed Plugins")
    t.add_column("Plugin", style="bold")
    t.add_column("Source")
    t.add_column("Components")
    t.add_column("Installed")
    for p in items:
        comps = p.get("components", [])
        if type_filter:
            comps = [c for c in comps if c["type"] in type_filter]
        summary = ", ".join(f"{c['type']}:{c['name']}" for c in comps)
        t.add_row(p["plugin_name"], p["source_name"], summary, p.get("installed_at", "")[:10])
    console.print(t)


def _list_plugins(plugins: list[dict], type_filter: set | None):
    installed_names = {p["plugin_name"] for p in registry.get_installed()}

    t = Table(title="Available Plugins")
    t.add_column("Name", style="bold")
    t.add_column("Description", max_width=60)
    t.add_column("Category")
    t.add_column("Status", justify="center")

    for p in plugins:
        name = p["name"]
        lp = p.get("_local_path")
        if name in installed_names:
            status = "[green]✓[/]"
        elif lp and Path(lp).is_dir():
            status = "[green]○[/]"
        elif isinstance(p.get("source"), dict):
            status = "[yellow]○[/]"
        else:
            status = "[red]✗[/]"
        t.add_row(name, p.get("description", "")[:60], p.get("category", ""), status)
    console.print(t)
    installed_count = sum(1 for p in plugins if p["name"] in installed_names)
    local_count = sum(1 for p in plugins if p.get("_local_path") and Path(p["_local_path"]).is_dir())
    fetch_count = sum(1 for p in plugins if not (p.get("_local_path") and Path(p["_local_path"]).is_dir()) and isinstance(p.get("source"), dict))
    console.print(f"\n[dim]{len(plugins)} plugins: [green]{installed_count} installed ✓[/], [green]{local_count} available ○[/], [yellow]{fetch_count} fetchable ○[/][/]")


def _show_plugin_detail(source_name: str, plugins: list[dict], name: str, type_filter: set | None):
    plugin = next((p for p in plugins if p["name"] == name), None)
    if not plugin:
        raise click.ClickException(f"Plugin '{name}' not found")

    installed = registry.get_installed_plugin(name)
    status = " [green](installed)[/]" if installed else ""
    console.print(f"\n[bold]{name}[/]{status}")
    console.print(f"  {plugin.get('description', '')}")

    local_path = plugin.get("_local_path")
    if not local_path or not Path(local_path).is_dir():
        src = plugin.get("source", "")
        if isinstance(src, dict):
            src_type = src.get("source", "")
            src_detail = src.get("url", "") or src.get("repo", "")
            console.print(f"\n  [dim]External ({src_type}): {src_detail}[/]")
            console.print(f"  [dim]Run 'kiro-cc add {name}' to fetch and install.[/]")
        else:
            console.print(f"\n  [yellow]⚠ Not available locally[/]")
        return

    components = scanner.scan_plugin(Path(local_path), plugin.get("_skill_filter"))
    if type_filter:
        components = [c for c in components if c.type in type_filter]

    if not components:
        console.print("  [dim]No convertible components found.[/]")
        return

    t = Table()
    t.add_column("Type", style="cyan")
    t.add_column("Name", style="bold")
    t.add_column("Description", max_width=50)
    for c in components:
        desc = c.frontmatter.get("description", "")[:50]
        t.add_row(c.type, c.name, desc)
    console.print(t)

    if installed:
        agents = [c for c in components if c.type in ("agent", "command")]
        if agents:
            console.print(f"\n[dim]Run with:[/]")
            for a in agents:
                console.print(f"  kiro-cli chat --agent {a.name}")


# ── add ──────────────────────────────────────────────────────────────────

@cli.command("add")
@click.argument("plugin_name", required=False)
@click.option("--source", "source_name", default=None, help="Source name")
@click.option("--all", "install_all", is_flag=True, help="Install all local plugins")
@click.option("--only", "only_types", default=None, help="Filter: skill,agent,command,mcp")
@click.option("--git", "git_url", default=None, help="Install from a git URL directly")
@click.option("--scope", default="global", type=click.Choice(["global", "workspace"]), help="Install scope")
def add_cmd(plugin_name: str | None, source_name: str | None, install_all: bool, only_types: str | None, git_url: str | None, scope: str):
    """Convert and install plugins to ~/.kiro (global) or .kiro/ (workspace)."""
    converter.set_scope(scope)
    type_filter = set(only_types.split(",")) if only_types else None

    if git_url:
        s = src_mod.add_source(git_url)
        source_name = s.name
        install_all = True

    # Auto-find source for a specific plugin name
    if plugin_name and not source_name:
        found = _find_plugin_source(plugin_name)
        if found:
            source_name, plugins = found
        else:
            source_name = _resolve_source(None)
            plugins = src_mod.parse_marketplace(source_name)
    else:
        source_name = _resolve_source(source_name)
        plugins = src_mod.parse_marketplace(source_name)

    commit = _get_commit(source_name)

    if install_all:
        targets = [p for p in plugins if p.get("_local_path") and Path(p["_local_path"]).is_dir()]
    elif plugin_name:
        targets = [p for p in plugins if p["name"] == plugin_name]
    else:
        raise click.ClickException("Specify a plugin name or --all")

    if not targets:
        raise click.ClickException("No matching plugins found")

    total_components = 0
    for plugin in targets:
        local_path = plugin.get("_local_path")

        # Auto-fetch external plugins if not local yet
        if (not local_path or not Path(local_path).is_dir()) and isinstance(plugin.get("source"), dict):
            with console.status(f"Fetching {plugin['name']}..."):
                fetched = src_mod.fetch_external_plugin(plugin["name"], plugin["source"])
            if fetched and fetched.is_dir():
                local_path = str(fetched)
            else:
                console.print(f"  [yellow]⚠ Failed to fetch {plugin['name']}[/]")
                continue
        elif not local_path or not Path(local_path).is_dir():
            console.print(f"  [yellow]⚠ Skipping {plugin['name']} (not available)[/]")
            continue

        components = scanner.scan_plugin(Path(local_path), plugin.get("_skill_filter"))
        if type_filter:
            components = [c for c in components if c.type in type_filter]

        if not components:
            continue

        converted = []
        for comp in components:
            conflict = converter.check_conflict(comp.name, comp.type, plugin["name"], source_name)
            if conflict:
                console.print(f"  [yellow]⚠ Skipping {comp.type}:{comp.name} — {conflict}[/]")
                continue
            result = converter.convert_component(comp, plugin["name"], source_name)
            if result:
                converted.append(result)
                total_components += 1

        if converted:
            registry.add_installed(plugin["name"], source_name, converted, commit, scope)
            types_summary = ", ".join(sorted(set(c["type"] for c in converted)))
            console.print(f"  [green]✓[/] {plugin['name']} → {len(converted)} components ({types_summary})")

    target_dir = "~/.kiro/" if scope == "global" else ".kiro/"
    console.print(f"\n[green]Done.[/] {total_components} components installed to {target_dir}")

    # Collect all agent names from this batch
    all_agents = []
    for plugin in targets:
        p_installed = registry.get_installed_plugin(plugin["name"])
        if p_installed:
            for c in p_installed.get("components", []):
                if c["type"] in ("agent", "command"):
                    all_agents.append(c["name"])
    if all_agents:
        console.print(f"\n[dim]Run with:[/]")
        for name in all_agents:
            console.print(f"  kiro-cli chat --agent {name}")


# ── delete ───────────────────────────────────────────────────────────────

@cli.command("delete")
@click.argument("plugin_name", required=False)
@click.option("--all", "delete_all", is_flag=True, help="Delete all installed plugins")
@click.option("--yes", "-y", is_flag=True, help="Skip confirmation")
def delete_cmd(plugin_name: str | None, delete_all: bool, yes: bool):
    """Remove converted plugins from ~/.kiro."""
    if delete_all:
        targets = [p["plugin_name"] for p in registry.get_installed()]
    elif plugin_name:
        targets = [plugin_name]
    else:
        raise click.ClickException("Specify a plugin name or --all")

    # Check all exist before confirming
    for name in targets:
        if not registry.get_installed_plugin(name):
            installed_names = [f"  {p['plugin_name']} (source: {p['source_name']})" for p in registry.get_installed()]
            msg = f"Plugin '{name}' is not installed."
            if installed_names:
                msg += "\nInstalled plugins:\n" + "\n".join(installed_names)
            raise click.ClickException(msg)

    if not yes:
        names = ", ".join(targets)
        click.confirm(f"Delete {names}?", abort=True)

    for name in targets:
        installed = registry.get_installed_plugin(name)
        converter.set_scope(installed.get("scope", "global"))
        components = registry.remove_installed(name)
        for comp in components:
            converter.remove_converted(comp["target_path"])
        console.print(f"  [green]✓[/] {name} (source: {installed['source_name']}) removed ({len(components)} components)")


# ── update ───────────────────────────────────────────────────────────────

@cli.command("update")
@click.argument("plugin_name", required=False)
@click.option("--all", "update_all", is_flag=True, help="Update all installed plugins")
@click.option("--only", "only_types", default=None, help="Filter: skill,agent,command,mcp")
def update_cmd(plugin_name: str | None, update_all: bool, only_types: str | None):
    """Re-sync from source and re-convert."""
    type_filter = set(only_types.split(",")) if only_types else None
    installed = registry.get_installed()
    if update_all:
        targets = installed
    elif plugin_name:
        targets = [p for p in installed if p["plugin_name"] == plugin_name]
    else:
        raise click.ClickException("Specify a plugin name or --all")

    if not targets:
        raise click.ClickException("No matching installed plugins")

    # Update sources
    updated_sources = set()
    for p in targets:
        sn = p["source_name"]
        if sn not in updated_sources:
            with console.status(f"Updating source {sn}..."):
                src_mod.update_source(sn)
            updated_sources.add(sn)

    # Re-convert
    for p in targets:
        sn = p["source_name"]
        scope = p.get("scope", "global")
        converter.set_scope(scope)
        plugins = src_mod.parse_marketplace(sn)
        plugin = next((pl for pl in plugins if pl["name"] == p["plugin_name"]), None)
        if not plugin or not plugin.get("_local_path"):
            console.print(f"  [yellow]⚠ {p['plugin_name']} not found in source[/]")
            continue

        # Remove old
        for comp in p.get("components", []):
            converter.remove_converted(comp["target_path"])

        # Re-scan and convert
        local_path = Path(plugin["_local_path"])
        components = scanner.scan_plugin(local_path, plugin.get("_skill_filter"))
        if type_filter:
            components = [c for c in components if c.type in type_filter]
        commit = _get_commit(sn)

        converted = []
        for comp in components:
            conflict = converter.check_conflict(comp.name, comp.type, p["plugin_name"], sn)
            if conflict:
                console.print(f"  [yellow]⚠ Skipping {comp.type}:{comp.name} — {conflict}[/]")
                continue
            result = converter.convert_component(comp, p["plugin_name"], sn)
            if result:
                converted.append(result)

        registry.add_installed(p["plugin_name"], sn, converted, commit, scope)
        console.print(f"  [green]✓[/] {p['plugin_name']} updated ({len(converted)} components)")
