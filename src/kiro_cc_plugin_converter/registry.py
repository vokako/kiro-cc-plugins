"""Registry: track installed conversions."""

from __future__ import annotations

from datetime import datetime
from pathlib import Path

from .models import REGISTRY_FILE, load_json, save_json


def _load() -> dict:
    return load_json(REGISTRY_FILE, {"installed": []})


def _save(data: dict):
    save_json(REGISTRY_FILE, data)


def add_installed(plugin_name: str, source_name: str, components: list[dict], commit: str):
    reg = _load()
    reg["installed"] = [p for p in reg["installed"] if p["plugin_name"] != plugin_name]
    reg["installed"].append({
        "plugin_name": plugin_name,
        "source_name": source_name,
        "components": components,
        "installed_at": datetime.now().isoformat(),
        "source_commit": commit,
    })
    _save(reg)


def remove_installed(plugin_name: str) -> list[dict]:
    """Remove and return the plugin's components."""
    reg = _load()
    removed = []
    remaining = []
    for p in reg["installed"]:
        if p["plugin_name"] == plugin_name:
            removed = p.get("components", [])
        else:
            remaining.append(p)
    reg["installed"] = remaining
    _save(reg)
    return removed


def get_installed() -> list[dict]:
    return _load().get("installed", [])


def get_installed_plugin(name: str) -> dict | None:
    for p in get_installed():
        if p["plugin_name"] == name:
            return p
    return None
