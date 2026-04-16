"""Convert Claude Code plugin components to Kiro format."""

from __future__ import annotations

import json
import shutil
from pathlib import Path

from .models import KIRO_HOME, REGISTRY_FILE, load_json, kiro_root
from .scanner import ScannedComponent

_scope = "global"


def set_scope(scope: str):
    global _scope
    _scope = scope


def _kiro_agent_path(name: str) -> Path:
    return kiro_root(_scope) / "agents" / f"{name}.json"


def _kiro_skill_dir(name: str, plugin_name: str, source_name: str) -> Path:
    safe_source = source_name.replace("/", "--")
    return kiro_root(_scope) / "skills" / f"{safe_source}--{plugin_name}--{name}"


def _is_ours(target: Path) -> bool:
    """Check if an existing file was created by us (has [from cc:] marker)."""
    if not target.exists():
        return False
    try:
        d = json.loads(target.read_text())
        return "[from cc:" in d.get("description", "")
    except Exception:
        return False


def _is_our_skill(target_dir: Path) -> bool:
    """Check if a skill dir was installed by us (in registry)."""
    reg = load_json(REGISTRY_FILE, {"installed": []})
    target_str = str(target_dir)
    for p in reg.get("installed", []):
        for c in p.get("components", []):
            if target_str in c.get("target_path", ""):
                return True
    return False


def check_conflict(name: str, comp_type: str, plugin_name: str = "", source_name: str = "") -> str | None:
    """Return conflict message if target already exists and isn't ours. None if ok."""
    if comp_type in ("agent", "command", "mcp"):
        suffix = f"{name}-mcp" if comp_type == "mcp" else name
        target = _kiro_agent_path(suffix)
        if target.exists() and not _is_ours(target):
            return f"Agent '{suffix}' already exists at {target}"
    elif comp_type == "skill":
        target = _kiro_skill_dir(name, plugin_name, source_name)
        if target.is_dir() and not _is_our_skill(target):
            return f"Skill '{name}' already exists at {target}"
    return None


def _map_tools(fm: dict) -> list[str]:
    """Map Claude Code tool names to Kiro tool names."""
    cc_tools = fm.get("tools", "")
    if not cc_tools:
        return ["*"]
    if isinstance(cc_tools, str):
        cc_tools = [t.strip() for t in cc_tools.split(",")]

    mapping = {
        "Read": "fs_read", "LS": "fs_read", "NotebookRead": "fs_read",
        "Write": "fs_write", "Edit": "fs_write", "NotebookEdit": "fs_write", "TodoWrite": "fs_write",
        "Glob": "glob", "Grep": "grep",
        "Bash": "execute_bash", "BashOutput": "execute_bash", "KillShell": "execute_bash",
        "Monitor": "execute_bash", "PowerShell": "execute_bash",
        "WebFetch": "web_fetch", "WebSearch": "web_search",
        "Agent": "use_subagent",
        "LSP": "code",
    }
    kiro_tools = set()
    for t in cc_tools:
        base = t.split("(")[0].strip()
        if base in mapping:
            kiro_tools.add(mapping[base])
        else:
            kiro_tools.add(base.lower())
    return sorted(kiro_tools) if kiro_tools else ["*"]


def convert_agent(comp: ScannedComponent, plugin_name: str) -> dict:
    """Convert a Claude Code agent .md to Kiro agent .json."""
    fm = comp.frontmatter

    desc = fm.get("description", "")
    if not desc:
        desc = f"[from cc:{plugin_name}]"
    else:
        desc = f"{desc} [from cc:{plugin_name}]"

    agent_config = {
        "$schema": "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json",
        "name": comp.name,
        "description": desc,
        "prompt": comp.body.strip(),
        "tools": _map_tools(fm),
        "allowedTools": [],
        "resources": [],
        "hooks": {},
        "toolsSettings": {},
    }

    if fm.get("model"):
        model_map = {"opus": "claude-opus-4", "sonnet": "claude-sonnet-4", "haiku": "claude-haiku-3.5"}
        agent_config["model"] = model_map.get(fm["model"], fm["model"])

    target = _kiro_agent_path(comp.name)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(agent_config, indent=2, ensure_ascii=False) + "\n")
    return {"type": comp.type, "name": comp.name, "source_rel": comp.rel_path, "target_path": str(target)}


def convert_skill(comp: ScannedComponent, plugin_name: str, source_name: str = "") -> dict:
    """Convert a Claude Code skill to Kiro skill (copy SKILL.md + supporting files)."""
    target_dir = _kiro_skill_dir(comp.name, plugin_name, source_name)
    target_dir.mkdir(parents=True, exist_ok=True)

    src_dir = comp.path.parent
    for item in src_dir.iterdir():
        dest = target_dir / item.name
        if item.is_dir():
            if dest.exists():
                shutil.rmtree(dest)
            shutil.copytree(item, dest)
        else:
            shutil.copy2(item, dest)

    target = target_dir / "SKILL.md"
    return {"type": "skill", "name": comp.name, "source_rel": comp.rel_path, "target_path": str(target)}


def convert_command(comp: ScannedComponent, plugin_name: str) -> dict:
    return convert_agent(comp, plugin_name)


def convert_mcp(comp: ScannedComponent, plugin_name: str) -> dict:
    mcp_data = comp.frontmatter
    mcp_name = f"{plugin_name}-mcp"

    agent_config = {
        "$schema": "https://raw.githubusercontent.com/aws/amazon-q-developer-cli/refs/heads/main/schemas/agent-v1.json",
        "name": mcp_name,
        "description": f"MCP servers [from cc:{plugin_name}]",
        "prompt": None,
        "mcpServers": {},
        "tools": ["*"],
        "allowedTools": [],
        "resources": [],
        "hooks": {},
        "toolsSettings": {},
    }

    for server_name, server_config in mcp_data.items():
        kiro_server = {}
        stype = server_config.get("type", "")
        if stype in ("http", "streamable-http", "sse"):
            kiro_server["type"] = "streamable-http" if stype == "http" else stype
            kiro_server["url"] = server_config.get("url", "")
        elif stype == "stdio":
            kiro_server["type"] = "stdio"
            kiro_server["command"] = server_config.get("command", "")
            if server_config.get("args"):
                kiro_server["args"] = server_config["args"]
        else:
            kiro_server = server_config
        agent_config["mcpServers"][server_name] = kiro_server

    target = _kiro_agent_path(mcp_name)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(agent_config, indent=2, ensure_ascii=False) + "\n")
    return {"type": "mcp", "name": mcp_name, "source_rel": ".mcp.json", "target_path": str(target)}


CONVERTERS = {
    "skill": convert_skill,
    "command": convert_command,
    "agent": convert_agent,
    "mcp": convert_mcp,
}


def convert_component(comp: ScannedComponent, plugin_name: str, source_name: str = "") -> dict | None:
    converter = CONVERTERS.get(comp.type)
    if converter:
        if comp.type == "skill":
            return converter(comp, plugin_name, source_name)
        return converter(comp, plugin_name)
    return None


def remove_converted(target_path: str):
    """Remove a converted file/directory. For skills, remove the whole skill dir."""
    p = Path(target_path)
    if p.is_dir():
        shutil.rmtree(p)
    elif p.exists():
        parent = p.parent
        p.unlink()
        # If this was a SKILL.md inside a skill dir, remove the whole dir
        if parent.parent == KIRO_HOME / "skills":
            shutil.rmtree(parent, ignore_errors=True)
