"""JSON API for GUI sidecar. Reads command from argv, writes JSON to stdout."""

from __future__ import annotations

import json
import sys
import traceback
from pathlib import Path

from . import source as src_mod
from . import scanner, converter, registry
from .models import ensure_dirs


def _serialize(obj):
    """Make objects JSON-serializable."""
    if isinstance(obj, Path):
        return str(obj)
    if hasattr(obj, "__dict__"):
        return obj.__dict__
    return str(obj)


def ok(data=None):
    return {"success": True, "data": data}


def err(msg: str):
    return {"success": False, "error": msg}


# ── helpers ───────────────────────────────────────────────────────────────

def _find_plugin(name: str) -> tuple[str, dict] | None:
    """Find a plugin by name across all sources. Returns (source_name, plugin) or None."""
    for s in src_mod.list_sources():
        try:
            plugins = src_mod.parse_marketplace(s["name"])
        except FileNotFoundError:
            continue
        plugin = next((p for p in plugins if p["name"] == name), None)
        if plugin:
            return s["name"], plugin
    return None


def _resolve_source(plugin_name: str | None, source_name: str | None) -> tuple[str, list[dict]]:
    """Resolve source name and return (source_name, plugins). Raises ValueError on failure."""
    if plugin_name and not source_name:
        found = _find_plugin(plugin_name)
        if found:
            source_name = found[0]
        else:
            sources = src_mod.list_sources()
            if len(sources) == 1:
                source_name = sources[0]["name"]
            else:
                raise ValueError(f"Plugin '{plugin_name}' not found in any source")
    if not source_name:
        sources = src_mod.list_sources()
        if len(sources) == 1:
            source_name = sources[0]["name"]
        else:
            raise ValueError("Multiple sources. Specify source.")
    return source_name, src_mod.parse_marketplace(source_name)


def _get_commit(source_name: str) -> str:
    for s in src_mod.list_sources():
        if s["name"] == source_name:
            return s.get("commit", "")
    return ""


# ── handlers ─────────────────────────────────────────────────────────────

def source_list():
    return ok(src_mod.list_sources())


def source_add(args: dict):
    s = src_mod.add_source(args["url"], args.get("name"))
    plugins = src_mod.parse_marketplace(s.name)
    return ok({"source": {"name": s.name, "url": s.url, "commit": s.commit}, "plugin_count": len(plugins)})


def source_remove(args: dict):
    name = args.get("name")
    force = args.get("force", False)
    remove_all = args.get("all", False)

    if remove_all:
        if not force:
            return err("--all requires force=true")
        targets = [s["name"] for s in src_mod.list_sources()]
    elif name:
        targets = [name]
    else:
        return err("Specify a source name or all=true")

    results = []
    for sn in targets:
        installed = [p for p in registry.get_installed() if p["source_name"] == sn]
        if installed and not force:
            names = [p["plugin_name"] for p in installed]
            return err(f"Source '{sn}' has {len(installed)} installed plugin(s): {', '.join(names)}. Use force=true to uninstall them.")
        uninstalled = []
        for p in installed:
            converter.set_scope(p.get("scope", "global"))
            components = registry.remove_installed(p["plugin_name"])
            for comp in components:
                converter.remove_converted(comp["target_path"], comp.get("mcp_keys"))
            uninstalled.append(p["plugin_name"])
        src_mod.remove_source(sn)
        results.append({"name": sn, "uninstalled_plugins": uninstalled})
    return ok(results)


def source_update(args: dict):
    name = args.get("name")
    results = []
    if name:
        commit = src_mod.update_source(name)
        results.append({"name": name, "commit": commit})
    else:
        for s in src_mod.list_sources():
            commit = src_mod.update_source(s["name"])
            results.append({"name": s["name"], "commit": commit})
    return ok(results)


def plugin_list(args: dict):
    source_name = args.get("source")
    installed_only = args.get("installed", False)
    only_types = args.get("only_types")

    if installed_only:
        items = registry.get_installed()
        if only_types:
            for p in items:
                p["components"] = [c for c in p.get("components", []) if c["type"] in only_types]
        return ok(items)

    all_plugins = []
    if source_name:
        all_plugins = src_mod.parse_marketplace(source_name)
    else:
        for s in src_mod.list_sources():
            try:
                all_plugins.extend(src_mod.parse_marketplace(s["name"]))
            except FileNotFoundError:
                continue

    installed_names = {p["plugin_name"] for p in registry.get_installed()}

    result = []
    for p in all_plugins:
        lp = p.get("_local_path")
        if p["name"] in installed_names:
            status = "installed"
        elif lp and Path(lp).is_dir():
            status = "available"
        elif isinstance(p.get("source"), dict):
            status = "fetchable"
        else:
            status = "unavailable"

        entry = {"name": p["name"], "description": p.get("description", ""), "category": p.get("category", ""), "status": status}

        # Include components if local
        if lp and Path(lp).is_dir():
            comps = scanner.scan_plugin(Path(lp), p.get("_skill_filter"))
            if only_types:
                comps = [c for c in comps if c.type in only_types]
            entry["components"] = [{"type": c.type, "name": c.name, "description": c.frontmatter.get("description", "")} for c in comps]

        result.append(entry)
    return ok(result)


def plugin_detail(args: dict):
    name = args["name"]
    found = _find_plugin(name)
    if not found:
        return err(f"Plugin '{name}' not found")

    source_name, plugin = found
    installed = registry.get_installed_plugin(name)
    lp = plugin.get("_local_path")
    components = []
    if lp and Path(lp).is_dir():
        comps = scanner.scan_plugin(Path(lp), plugin.get("_skill_filter"))
        components = [{"type": c.type, "name": c.name, "description": c.frontmatter.get("description", "")} for c in comps]

    return ok({
        "name": name,
        "description": plugin.get("description", ""),
        "category": plugin.get("category", ""),
        "source": source_name,
        "installed": installed is not None,
        "components": components,
    })


def plugin_add(args: dict):
    plugin_name = args.get("name")
    source_name = args.get("source")
    scope = args.get("scope", "global")
    only_types = set(args["only_types"]) if args.get("only_types") else None
    install_all = args.get("all", False)

    converter.set_scope(scope)

    try:
        source_name, plugins = _resolve_source(plugin_name, source_name)
    except ValueError as e:
        return err(str(e))

    commit = _get_commit(source_name)

    if install_all:
        targets = [p for p in plugins if p.get("_local_path") and Path(p["_local_path"]).is_dir()]
    elif plugin_name:
        targets = [p for p in plugins if p["name"] == plugin_name]
    else:
        return err("Specify a plugin name or all=true")

    results = []
    for plugin in targets:
        local_path = plugin.get("_local_path")
        if (not local_path or not Path(local_path).is_dir()) and isinstance(plugin.get("source"), dict):
            fetched = src_mod.fetch_external_plugin(plugin["name"], plugin["source"])
            if fetched and fetched.is_dir():
                local_path = str(fetched)
            else:
                results.append({"name": plugin["name"], "status": "fetch_failed"})
                continue
        elif not local_path or not Path(local_path).is_dir():
            results.append({"name": plugin["name"], "status": "unavailable"})
            continue

        components = scanner.scan_plugin(Path(local_path), plugin.get("_skill_filter"))
        if only_types:
            components = [c for c in components if c.type in only_types]

        converted = []
        skipped = []
        for comp in components:
            conflict = converter.check_conflict(comp.name, comp.type, plugin["name"], source_name)
            if conflict:
                skipped.append({"type": comp.type, "name": comp.name, "reason": conflict})
                continue
            result = converter.convert_component(comp, plugin["name"], source_name)
            if result:
                converted.append(result)

        if converted:
            registry.add_installed(plugin["name"], source_name, converted, commit, scope)

        results.append({"name": plugin["name"], "status": "installed", "components": len(converted), "skipped": skipped})

    return ok(results)


def plugin_delete(args: dict):
    plugin_name = args.get("name")
    delete_all = args.get("all", False)

    if delete_all:
        targets = [p["plugin_name"] for p in registry.get_installed()]
    elif plugin_name:
        targets = [plugin_name]
    else:
        return err("Specify a plugin name or all=true")

    results = []
    for name in targets:
        installed = registry.get_installed_plugin(name)
        if not installed:
            results.append({"name": name, "status": "not_installed"})
            continue
        converter.set_scope(installed.get("scope", "global"))
        components = registry.remove_installed(name)
        for comp in components:
            converter.remove_converted(comp["target_path"], comp.get("mcp_keys"))
        results.append({"name": name, "status": "deleted", "components": len(components)})

    return ok(results)


def plugin_update(args: dict):
    plugin_name = args.get("name")
    update_all = args.get("all", False)
    only_types = set(args["only_types"]) if args.get("only_types") else None

    installed = registry.get_installed()
    if update_all:
        targets = installed
    elif plugin_name:
        targets = [p for p in installed if p["plugin_name"] == plugin_name]
    else:
        return err("Specify a plugin name or all=true")

    if not targets:
        return err("No matching installed plugins")

    # Update sources first
    updated_sources = set()
    for p in targets:
        sn = p["source_name"]
        if sn not in updated_sources:
            src_mod.update_source(sn)
            updated_sources.add(sn)

    results = []
    for p in targets:
        sn = p["source_name"]
        scope = p.get("scope", "global")
        converter.set_scope(scope)
        plugins = src_mod.parse_marketplace(sn)
        plugin = next((pl for pl in plugins if pl["name"] == p["plugin_name"]), None)
        if not plugin or not plugin.get("_local_path"):
            results.append({"name": p["plugin_name"], "status": "not_found"})
            continue

        for comp in p.get("components", []):
            converter.remove_converted(comp["target_path"], comp.get("mcp_keys"))

        local_path = Path(plugin["_local_path"])
        components = scanner.scan_plugin(local_path, plugin.get("_skill_filter"))
        if only_types:
            components = [c for c in components if c.type in only_types]

        commit = _get_commit(sn)

        converted = []
        for comp in components:
            conflict = converter.check_conflict(comp.name, comp.type, p["plugin_name"], sn)
            if conflict:
                continue
            result = converter.convert_component(comp, p["plugin_name"], sn)
            if result:
                converted.append(result)

        registry.add_installed(p["plugin_name"], sn, converted, commit, scope)
        results.append({"name": p["plugin_name"], "status": "updated", "components": len(converted)})

    return ok(results)


# ── router ───────────────────────────────────────────────────────────────

COMMANDS = {
    "source.list": source_list,
    "source.add": source_add,
    "source.remove": source_remove,
    "source.update": source_update,
    "plugin.list": plugin_list,
    "plugin.detail": plugin_detail,
    "plugin.add": plugin_add,
    "plugin.delete": plugin_delete,
    "plugin.update": plugin_update,
}


def main():
    ensure_dirs()
    if len(sys.argv) < 2:
        print(json.dumps(err("Usage: api <json>")), flush=True)
        sys.exit(1)

    try:
        request = json.loads(sys.argv[1])
        cmd = request.get("command", "")
        args = request.get("args", {})

        handler = COMMANDS.get(cmd)
        if not handler:
            result = err(f"Unknown command: {cmd}")
        else:
            import inspect
            if inspect.signature(handler).parameters:
                result = handler(args)
            else:
                result = handler()

    except Exception as e:
        result = err(f"{type(e).__name__}: {e}")

    print(json.dumps(result, default=_serialize, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
