"""End-to-end integration test.

Covers the full flow: add source → install plugins → verify files → delete → verify cleanup.
Tests three plugin types: agent+command (feature-dev), skill (document-skills), mcp (context7).

Uses an isolated KIRO_HOME in /tmp so it doesn't touch the user's real config.
Requires network (git clone).

Run: python -m tests.test_e2e
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
from pathlib import Path


# ── isolation: set KIRO_HOME before importing the package ────────────────
TEST_HOME = Path(tempfile.mkdtemp(prefix="kiro-cc-test-"))
os.environ["HOME"] = str(TEST_HOME)

# Patch models paths to use the test home
from kiro_cc_plugin_converter import models  # noqa: E402

models.KIRO_HOME = TEST_HOME / ".kiro"
models.CONVERTER_HOME = models.KIRO_HOME / "cc-plugins"
models.CACHE_DIR = models.CONVERTER_HOME / "cache"
models.CONFIG_FILE = models.CONVERTER_HOME / "config.json"
models.REGISTRY_FILE = models.CONVERTER_HOME / "registry.json"

# Also patch the import references in other modules
from kiro_cc_plugin_converter import source as src_mod  # noqa: E402
from kiro_cc_plugin_converter import converter, registry, scanner  # noqa: E402

src_mod.CACHE_DIR = models.CACHE_DIR
src_mod.CONFIG_FILE = models.CONFIG_FILE
src_mod.EXTERNAL_CACHE = models.CACHE_DIR / "_external"
converter.REGISTRY_FILE = models.REGISTRY_FILE
converter.KIRO_HOME = models.KIRO_HOME
registry.REGISTRY_FILE = models.REGISTRY_FILE

models.ensure_dirs()


# ── test helpers ─────────────────────────────────────────────────────────

PASSED = 0
FAILED = 0


def check(cond: bool, msg: str):
    global PASSED, FAILED
    if cond:
        PASSED += 1
        print(f"  \033[32m✓\033[0m {msg}")
    else:
        FAILED += 1
        print(f"  \033[31m✗\033[0m {msg}")


def section(title: str):
    print(f"\n\033[1m== {title} ==\033[0m")


def kiro_agent_files() -> list[Path]:
    d = models.KIRO_HOME / "agents"
    return list(d.glob("*.json")) if d.is_dir() else []


def kiro_skill_dirs() -> list[Path]:
    d = models.KIRO_HOME / "skills"
    return [p for p in d.iterdir() if p.is_dir()] if d.is_dir() else []


def read_mcp_settings() -> dict:
    p = models.KIRO_HOME / "settings" / "mcp.json"
    if not p.exists():
        return {}
    return json.loads(p.read_text())


def validate_agent_json(path: Path) -> list[str]:
    """Validate an agent JSON matches Kiro's expected shape. Returns list of error messages."""
    errors = []
    try:
        d = json.loads(path.read_text())
    except Exception as e:
        return [f"invalid JSON: {e}"]

    required_str = ["name", "description"]
    for k in required_str:
        if not isinstance(d.get(k), str) or not d[k]:
            errors.append(f"missing/invalid '{k}'")

    # prompt can be str or None
    if "prompt" in d and d["prompt"] is not None and not isinstance(d["prompt"], str):
        errors.append("'prompt' must be string or null")

    for k in ["tools", "allowedTools", "resources"]:
        if not isinstance(d.get(k), list):
            errors.append(f"'{k}' must be list")

    for k in ["hooks", "toolsSettings"]:
        if not isinstance(d.get(k), dict):
            errors.append(f"'{k}' must be dict")

    # schema URL present
    if not d.get("$schema", "").startswith("http"):
        errors.append("missing/invalid $schema URL")

    return errors


def validate_skill_md(path: Path) -> list[str]:
    """Validate SKILL.md has YAML frontmatter with required fields."""
    errors = []
    text = path.read_text()
    if not text.startswith("---"):
        return ["missing YAML frontmatter"]
    import re
    import yaml
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n", text, re.DOTALL)
    if not m:
        return ["malformed frontmatter delimiters"]
    try:
        fm = yaml.safe_load(m.group(1)) or {}
    except Exception as e:
        return [f"YAML parse error: {e}"]
    if not fm.get("name"):
        errors.append("missing 'name' in frontmatter")
    if not fm.get("description"):
        errors.append("missing 'description' in frontmatter")
    return errors


def validate_mcp_server(cfg: dict) -> list[str]:
    """Validate an MCP server config matches Kiro's schema."""
    errors = []
    if not isinstance(cfg, dict):
        return ["not a dict"]
    stype = cfg.get("type")
    # Either stdio (default) with command, or http/streamable-http with url
    if stype in ("http", "streamable-http", "sse"):
        if not cfg.get("url"):
            errors.append(f"{stype} server missing 'url'")
    else:
        # stdio (explicit or implicit)
        if not cfg.get("command"):
            errors.append("stdio server missing 'command'")
        if "args" in cfg and not isinstance(cfg["args"], list):
            errors.append("'args' must be list")
        if "env" in cfg and not isinstance(cfg["env"], dict):
            errors.append("'env' must be dict")
    return errors


def install(plugin_name: str):
    """Install a plugin via converter directly (same path as CLI/API)."""
    found = None
    for s in src_mod.list_sources():
        plugins = src_mod.parse_marketplace(s["name"])
        p = next((x for x in plugins if x["name"] == plugin_name), None)
        if p:
            found = (s["name"], p)
            break
    assert found, f"Plugin {plugin_name} not found"
    source_name, plugin = found

    # Fetch external if needed
    local_path = plugin.get("_local_path")
    if (not local_path or not Path(local_path).is_dir()) and isinstance(plugin.get("source"), dict):
        fetched = src_mod.fetch_external_plugin(plugin["name"], plugin["source"])
        if fetched and fetched.is_dir():
            local_path = str(fetched)
    assert local_path and Path(local_path).is_dir(), f"Cannot resolve local path for {plugin_name}"

    components = scanner.scan_plugin(Path(local_path), plugin.get("_skill_filter"))
    converter.set_scope("global")
    converted = []
    for comp in components:
        result = converter.convert_component(comp, plugin_name, source_name)
        if result:
            converted.append(result)
    commit = next((s.get("commit", "") for s in src_mod.list_sources() if s["name"] == source_name), "")
    registry.add_installed(plugin_name, source_name, converted, commit)


def uninstall(plugin_name: str):
    inst = registry.get_installed_plugin(plugin_name)
    if not inst:
        return
    converter.set_scope(inst.get("scope", "global"))
    components = registry.remove_installed(plugin_name)
    for comp in components:
        converter.remove_converted(comp["target_path"], comp.get("mcp_keys"))


# ── the test ─────────────────────────────────────────────────────────────

def main():
    print(f"Test home: {TEST_HOME}")

    try:
        # --- SETUP: add sources ---
        section("Add sources")
        src_mod.add_source("https://github.com/anthropics/claude-plugins-official")
        src_mod.add_source("https://github.com/anthropics/skills.git")
        sources = src_mod.list_sources()
        check(len(sources) == 2, "2 sources registered")
        names = {s["name"] for s in sources}
        check("anthropics/claude-plugins-official" in names, "claude-plugins-official registered")
        check("anthropics/skills" in names, "anthropics/skills registered")

        # --- INSTALL feature-dev (command + agents) ---
        section("Install feature-dev (command + agents)")
        install("feature-dev")
        agent_files = kiro_agent_files()
        names = {f.stem for f in agent_files}
        expected = {
            "cc--anthropics--claude-plugins-official--feature-dev--feature-dev",
            "cc--anthropics--claude-plugins-official--feature-dev--code-reviewer",
            "cc--anthropics--claude-plugins-official--feature-dev--code-explorer",
            "cc--anthropics--claude-plugins-official--feature-dev--code-architect",
        }
        check(expected.issubset(names), "all 4 agent files created with namespaced names")
        # Validate every agent JSON against Kiro schema
        for f in agent_files:
            if "feature-dev" in f.stem:
                errs = validate_agent_json(f)
                check(not errs, f"{f.name}: schema valid" + (f" ({errs})" if errs else ""))
        # JSON internal name must be original (no prefix)
        for fname, inner in [
            ("cc--anthropics--claude-plugins-official--feature-dev--code-reviewer.json", "code-reviewer"),
            ("cc--anthropics--claude-plugins-official--feature-dev--feature-dev.json", "feature-dev"),
        ]:
            d = json.loads((models.KIRO_HOME / "agents" / fname).read_text())
            check(d["name"] == inner, f"{fname}: internal name = {inner}")
            check("[from cc:feature-dev]" in d["description"], f"{fname}: description has cc marker")
        check(registry.get_installed_plugin("feature-dev") is not None, "feature-dev in registry")

        # --- INSTALL document-skills (skills only, via skill_filter) ---
        section("Install document-skills (skills)")
        install("document-skills")
        skill_dirs = kiro_skill_dirs()
        skill_names = {d.name for d in skill_dirs}
        expected_skills = {
            "anthropics--skills--document-skills--xlsx",
            "anthropics--skills--document-skills--pdf",
            "anthropics--skills--document-skills--pptx",
            "anthropics--skills--document-skills--docx",
        }
        check(expected_skills.issubset(skill_names), "all 4 skill dirs created")
        # Each skill dir has SKILL.md with valid frontmatter
        for name in expected_skills:
            skill_md = models.KIRO_HOME / "skills" / name / "SKILL.md"
            check(skill_md.exists(), f"{name}/SKILL.md exists")
            if skill_md.exists():
                errs = validate_skill_md(skill_md)
                check(not errs, f"{name}/SKILL.md: frontmatter valid" + (f" ({errs})" if errs else ""))

        # --- INSTALL context7 (MCP, external fetch) ---
        section("Install context7 (MCP)")
        install("context7")
        mcp = read_mcp_settings()
        mcp_servers = mcp.get("mcpServers", {})
        key = "cc-context7-context7"
        check(key in mcp_servers, f"MCP key '{key}' present in settings/mcp.json")
        check(mcp_servers.get(key, {}).get("command") == "npx", "context7 command = npx")
        check("@upstash/context7-mcp" in mcp_servers.get(key, {}).get("args", []), "context7 args contain package")
        # Validate MCP server config schema
        errs = validate_mcp_server(mcp_servers.get(key, {}))
        check(not errs, f"context7 MCP schema valid" + (f" ({errs})" if errs else ""))

        # --- VERIFY registry state ---
        section("Verify registry state")
        installed = registry.get_installed()
        plugin_names = {p["plugin_name"] for p in installed}
        check(plugin_names == {"feature-dev", "document-skills", "context7"}, "registry has all 3 plugins")

        # --- UNINSTALL feature-dev ---
        section("Uninstall feature-dev")
        uninstall("feature-dev")
        names_after = {f.stem for f in kiro_agent_files()}
        check(not any("feature-dev" in n for n in names_after), "feature-dev agent files removed")
        check(registry.get_installed_plugin("feature-dev") is None, "feature-dev removed from registry")

        # --- UNINSTALL document-skills ---
        section("Uninstall document-skills")
        uninstall("document-skills")
        skill_names_after = {d.name for d in kiro_skill_dirs()}
        check(not any("document-skills" in n for n in skill_names_after), "document-skills dirs removed")

        # --- UNINSTALL context7 (MCP key removed from mcp.json) ---
        section("Uninstall context7")
        uninstall("context7")
        mcp_after = read_mcp_settings().get("mcpServers", {})
        check(key not in mcp_after, f"MCP key '{key}' removed from mcp.json")

        # --- FINAL: registry empty ---
        section("Final state")
        check(len(registry.get_installed()) == 0, "registry is empty")
        check(len(kiro_agent_files()) == 0, "no agent files remain")
        check(len(kiro_skill_dirs()) == 0, "no skill dirs remain")

    finally:
        shutil.rmtree(TEST_HOME, ignore_errors=True)

    print(f"\n\033[1m{PASSED} passed, {FAILED} failed\033[0m")
    sys.exit(0 if FAILED == 0 else 1)


if __name__ == "__main__":
    main()
