"""Data models."""

from __future__ import annotations

import json
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path

KIRO_HOME = Path.home() / ".kiro"
KIRO_WORKSPACE = Path(".kiro")
CONVERTER_HOME = KIRO_HOME / "cc-plugins"
CACHE_DIR = CONVERTER_HOME / "cache"
CONFIG_FILE = CONVERTER_HOME / "config.json"
REGISTRY_FILE = CONVERTER_HOME / "registry.json"


def kiro_root(scope: str = "global") -> Path:
    return KIRO_HOME if scope == "global" else KIRO_WORKSPACE


def ensure_dirs():
    """Create required directories if they don't exist."""
    CONVERTER_HOME.mkdir(parents=True, exist_ok=True)
    CACHE_DIR.mkdir(parents=True, exist_ok=True)

CC_PREFIX = ""


@dataclass
class Source:
    name: str
    url: str
    cloned_at: str = ""
    commit: str = ""


@dataclass
class Component:
    type: str  # skill, command, agent, mcp
    name: str
    source_rel: str  # relative path within plugin
    target_path: str  # absolute path of converted file


@dataclass
class InstalledPlugin:
    plugin_name: str
    source_name: str
    components: list[dict] = field(default_factory=list)
    installed_at: str = ""
    source_commit: str = ""


def load_json(path: Path, default=None):
    if path.exists():
        return json.loads(path.read_text())
    return default if default is not None else {}


def save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
