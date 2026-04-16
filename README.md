# kiro-cc-plugins

Install [Claude Code plugins](https://code.claude.com/docs/en/plugins) into [Kiro](https://kiro.dev).

Supports marketplace repos (like the [official Anthropic marketplace](https://github.com/anthropics/claude-plugins-official)) and standalone plugin/skill repos.

## Install

```bash
# One-shot (no install needed)
uvx --from git+https://github.com/anthropics/kiro-cc-plugins.git kiro-cc-plugins --help

# Or install permanently
uv tool install git+https://github.com/anthropics/kiro-cc-plugins.git
```

## Quick Start

```bash
# Add the official Claude Code marketplace
kiro-cc-plugins source add https://github.com/anthropics/claude-plugins-official

# Add the Anthropic skills repo
kiro-cc-plugins source add https://github.com/anthropics/skills.git

# Browse available plugins
kiro-cc-plugins list

# List all skills across sources
kiro-cc-plugins list --skills

# Install a plugin
kiro-cc-plugins add feature-dev

# Install to current project only
kiro-cc-plugins add feature-dev --scope workspace

# Install only specific component types
kiro-cc-plugins add plugin-dev --only skill

# Install from any git URL directly
kiro-cc-plugins add --git https://github.com/user/my-cc-plugin
```

## Commands

| Command | Description |
|---|---|
| `source add <url>` | Clone a marketplace or plugin repo |
| `source list` | List registered sources |
| `source update [name]` | Git pull sources |
| `source remove <name>` | Remove a source |
| `list [plugin]` | List available plugins or show plugin details |
| `list --installed` | Show installed plugins |
| `list --skills` | List all skills across sources |
| `list --agents` | List all agents across sources |
| `add <name>` | Convert and install a plugin |
| `add --all` | Install all local plugins |
| `add --scope workspace` | Install to `.kiro/` instead of `~/.kiro/` |
| `update [name] / --all` | Re-sync from source and re-convert |
| `delete <name> / --all` | Remove converted plugins |

## Conversion Mapping

| Claude Code | Kiro | Format |
|---|---|---|
| `skills/*/SKILL.md` | `~/.kiro/skills/{source}--{plugin}--{skill}/` | Direct copy (compatible format) |
| `commands/*.md` | `~/.kiro/agents/{name}.json` | Prompt → agent JSON |
| `agents/*.md` | `~/.kiro/agents/{name}.json` | Prompt → agent JSON, tools mapped |
| `.mcp.json` | `~/.kiro/agents/{plugin}-mcp.json` | MCP config → agent with mcpServers |

Hooks and LSP configs are skipped (no Kiro equivalent). Existing agents with the same name are not overwritten.
