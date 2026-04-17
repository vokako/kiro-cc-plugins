# kiro-cc-plugins

Install [Claude Code plugins](https://code.claude.com/docs/en/plugins) into [Kiro](https://kiro.dev).

Supports marketplace repos (like the [official Anthropic marketplace](https://github.com/anthropics/claude-plugins-official)) and standalone plugin/skill repos.

<img width="5120" height="2818" alt="app-installed" src="https://github.com/user-attachments/assets/ad466897-709b-4fb2-8cc7-397d1f46f031" />
<img width="5120" height="2818" alt="app-plugins" src="https://github.com/user-attachments/assets/bdd43418-7a16-4f8b-8e45-aaa8e8e0283b" />
<img width="5120" height="2818" alt="app-sources" src="https://github.com/user-attachments/assets/4fc43c94-562a-44c4-bd18-18c444c0c373" />

## Install

### CLI

```bash
# One-line install (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/vokako/kiro-cc-plugins/main/install.sh | bash

# Or build from source
cargo install --git https://github.com/vokako/kiro-cc-plugins kiro-cc-plugins
```

No system `git`, Python, or other runtime required — single self-contained binary.

### Desktop App

Download `.dmg` (macOS) or `.exe` (Windows) from the [releases page](https://github.com/vokako/kiro-cc-plugins/releases).

## Quick Start

```bash
# Add the official Claude Code marketplace
kiro-cc-plugins source add https://github.com/anthropics/claude-plugins-official

# Add the Anthropic skills repo
kiro-cc-plugins source add https://github.com/anthropics/skills.git

# Show overview of sources and installed plugins
kiro-cc-plugins status

# Browse all available plugins
kiro-cc-plugins list

# Show plugin details
kiro-cc-plugins list feature-dev

# Install a plugin (auto-detects which source)
kiro-cc-plugins add feature-dev

# Install only specific component types
kiro-cc-plugins add plugin-dev --only skill

# Install from any git URL directly
kiro-cc-plugins add --git https://github.com/user/my-cc-plugin

# Update installed plugins (git pull + re-convert)
kiro-cc-plugins update --all

# Delete a plugin
kiro-cc-plugins delete feature-dev
```

After installing agents, run them with:

```bash
kiro-cli chat --agent feature-dev
```

## Commands

| Command | Description |
|---|---|
| `status` | Show sources and installed plugins overview |
| `source add <url>` | Clone a marketplace or plugin repo |
| `source list` | List registered sources |
| `source update [name]` | Git pull sources (or all) |
| `source remove <name>` | Remove a source (refuses if plugins installed) |
| `source remove <name> --force` | Remove source and uninstall its plugins |
| `source remove --all --force` | Wipe all sources and plugins |
| `list` | List all plugins across all sources |
| `list <plugin>` | Show plugin details, install status, and run commands |
| `list --installed` | Show installed plugins with enable/disable status |
| `list --skills` | List all skills across sources |
| `list --agents` | List all agents across sources |
| `add <name>` | Install a plugin (auto-detects source) |
| `add --all` | Install all locally available plugins |
| `add --only skill,agent` | Install only specific component types |
| `add --scope workspace` | Install to `.kiro/` instead of `~/.kiro/` |
| `add --git <url>` | Install directly from a git URL |
| `update <name> / --all` | Git pull source and re-convert |
| `update --only skill` | Re-convert only specific component types |
| `delete <name> / --all` | Remove installed plugins (alias: `remove`) |
| `delete -y` | Skip confirmation prompt |
| `enable <plugin>` | Enable all components of a plugin |
| `enable <plugin> --component <name>` | Enable a specific component |
| `enable <plugin> --only skill` | Enable only skills |
| `disable <plugin>` | Disable all components of a plugin |
| `disable <plugin> --component <name>` | Disable a specific component |

## Conversion Mapping

| Claude Code | Kiro | Format |
|---|---|---|
| `skills/*/SKILL.md` | `~/.kiro/skills/{source}--{plugin}--{skill}/` | Direct copy |
| `commands/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` | Prompt → agent JSON |
| `agents/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` | Prompt → agent JSON, tools mapped |
| `.mcp.json` | `~/.kiro/settings/mcp.json` (key `cc-{plugin}-{server}`) | Merged into Kiro MCP settings |

Hooks and LSP configs are skipped (no Kiro equivalent).

## Enable / Disable

Components can be individually enabled or disabled without uninstalling:

- **Skills**: moved to `~/.kiro/cc-plugins/disabled-skills/` when disabled
- **Agents/Commands**: moved to `~/.kiro/cc-plugins/disabled-agents/` when disabled
- **MCP servers**: `"disabled": true` flag set in `~/.kiro/settings/mcp.json`

The desktop app shows toggle buttons on each component card.

## Status Icons

| Icon | Meaning |
|---|---|
| ✓ (green) | Installed & enabled |
| ✗ | Disabled |
| ○ (green) | Available locally |
| ○ (yellow) | Fetchable (external, auto-cloned on `add`) |

## Data Storage

```
~/.kiro/cc-plugins/
├── config.json          # Registered sources
├── registry.json        # Installed plugin tracking
├── cache/               # Cloned git repos
├── disabled-skills/     # Parked disabled skills
└── disabled-agents/     # Parked disabled agents
```

## Architecture

Cargo workspace with three crates:

- `crates/core/` — Core library (source, scanner, converter, registry, JSON API)
- `crates/cli/` — CLI binary (`kiro-cc-plugins`)
- `gui/` — Tauri + Svelte desktop app (uses `crates/core` as in-process SDK)

```bash
# Build everything
cargo build --release

# Run CLI
cargo run -p kiro-cc-plugins -- status

# Run tests
cargo test
cargo test -p kiro_cc_core --test e2e -- --ignored  # requires network

# Build desktop app
cd gui && npm install && npm run tauri build
```
