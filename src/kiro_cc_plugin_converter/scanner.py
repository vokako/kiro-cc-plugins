"""Scan Claude Code plugin directories for convertible components."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path


@dataclass
class ScannedComponent:
    type: str  # skill, command, agent, mcp
    name: str
    path: Path  # absolute path to the file
    rel_path: str  # relative to plugin root
    frontmatter: dict
    body: str


def _parse_frontmatter(text: str) -> tuple[dict, str]:
    """Parse YAML frontmatter from markdown."""
    import yaml

    m = re.match(r"^---\s*\n(.*?)\n---\s*\n(.*)", text, re.DOTALL)
    if not m:
        return {}, text
    try:
        fm = yaml.safe_load(m.group(1)) or {}
    except Exception:
        fm = {}
    return fm, m.group(2)


def scan_plugin(plugin_dir: Path, skill_filter: list[str] | None = None) -> list[ScannedComponent]:
    """Scan a Claude Code plugin directory for skills, commands, agents, mcp.
    
    skill_filter: if set, only include skills whose relative dirs match (e.g. ["./skills/pdf"]).
    """
    if not plugin_dir.is_dir():
        return []

    # Normalize skill_filter to a set of absolute paths
    allowed_skill_dirs: set[str] | None = None
    if skill_filter:
        allowed_skill_dirs = set()
        for sf in skill_filter:
            p = (plugin_dir / sf.lstrip("./")).resolve()
            allowed_skill_dirs.add(str(p))

    components = []

    # Skills: skills/*/SKILL.md
    skills_dir = plugin_dir / "skills"
    if skills_dir.is_dir():
        for skill_dir in skills_dir.iterdir():
            if not skill_dir.is_dir():
                continue
            if allowed_skill_dirs and str(skill_dir.resolve()) not in allowed_skill_dirs:
                continue
            skill_md = skill_dir / "SKILL.md"
            if skill_md.exists():
                text = skill_md.read_text()
                fm, body = _parse_frontmatter(text)
                # Collect supporting files
                components.append(ScannedComponent(
                    type="skill",
                    name=fm.get("name", skill_dir.name),
                    path=skill_md,
                    rel_path=str(skill_md.relative_to(plugin_dir)),
                    frontmatter=fm,
                    body=body,
                ))

    # Commands: commands/*.md
    cmds_dir = plugin_dir / "commands"
    if cmds_dir.is_dir():
        for md in cmds_dir.glob("*.md"):
            text = md.read_text()
            fm, body = _parse_frontmatter(text)
            components.append(ScannedComponent(
                type="command",
                name=fm.get("name", md.stem),
                path=md,
                rel_path=str(md.relative_to(plugin_dir)),
                frontmatter=fm,
                body=body,
            ))

    # Agents: agents/*.md
    agents_dir = plugin_dir / "agents"
    if agents_dir.is_dir():
        for md in agents_dir.glob("*.md"):
            text = md.read_text()
            fm, body = _parse_frontmatter(text)
            components.append(ScannedComponent(
                type="agent",
                name=fm.get("name", md.stem),
                path=md,
                rel_path=str(md.relative_to(plugin_dir)),
                frontmatter=fm,
                body=body,
            ))

    # MCP: .mcp.json
    mcp_file = plugin_dir / ".mcp.json"
    if mcp_file.exists():
        import json

        try:
            mcp_data = json.loads(mcp_file.read_text())
            components.append(ScannedComponent(
                type="mcp",
                name="mcp",
                path=mcp_file,
                rel_path=".mcp.json",
                frontmatter=mcp_data,
                body="",
            ))
        except Exception:
            pass

    return components
