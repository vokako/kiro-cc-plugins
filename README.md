# kiro-cc-plugins

Install [Claude Code plugins](https://code.claude.com/docs/en/plugins) into [Kiro](https://kiro.dev).

Supports marketplace repos (like the [official Anthropic marketplace](https://github.com/anthropics/claude-plugins-official)) and standalone plugin/skill repos.

https://github.com/user-attachments/assets/81dde49f-8632-4427-b6d5-9492d5da09b8

## Install

### CLI

```bash
# Build from source (requires Rust toolchain)
cargo install --git https://github.com/vokako/kiro-cc-plugins kiro-cc-plugins

# Or clone and build locally
git clone https://github.com/vokako/kiro-cc-plugins
cd kiro-cc-plugins
cargo install --path crates/cli
```

The binary is self-contained — no system `git`, Python, or other runtime required.

### Desktop App

Download a prebuilt `.dmg` (macOS) or `.exe` (Windows) installer from the [releases page](https://github.com/vokako/kiro-cc-plugins/releases).

## Quick Start

```bash
# Add the official Claude Code marketplace
kiro-cc-plugins source add https://github.com/anthropics/claude-plugins-official

# Add the Anthropic skills repo
kiro-cc-plugins source add https://github.com/anthropics/skills.git

# Browse all available plugins (merges all sources)
kiro-cc-plugins list

# List all skills or agents across sources
kiro-cc-plugins list --skills
kiro-cc-plugins list --agents

# Show plugin details (shows install status and run commands)
kiro-cc-plugins list feature-dev

# Install a plugin (auto-detects which source)
kiro-cc-plugins add feature-dev

# Install to current project only
kiro-cc-plugins add feature-dev --scope workspace

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
| `source add <url>` | Clone a marketplace or plugin repo |
| `source list` | List registered sources |
| `source update [name]` | Git pull sources |
| `source remove <name>` | Remove a source (refuses if plugins installed) |
| `source remove <name> --force` | Remove a source and its installed plugins |
| `source remove --all --force` | Wipe all sources and their plugins |
| `list` | List all plugins across all sources |
| `list <plugin>` | Show plugin details, install status, and run commands |
| `list --installed` | Show installed plugins only |
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

## Conversion Mapping

| Claude Code | Kiro | Format |
|---|---|---|
| `skills/*/SKILL.md` | `~/.kiro/skills/{source}--{plugin}--{skill}/` | Direct copy |
| `commands/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` | Prompt → agent JSON |
| `agents/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` | Prompt → agent JSON, tools mapped |
| `.mcp.json` | `~/.kiro/settings/mcp.json` (key `cc-{plugin}-{server}`) | Merged into Kiro MCP settings |

Hooks and LSP configs are skipped (no Kiro equivalent). Existing agents with the same namespaced filename are not overwritten.

## Status Icons

| Icon | Meaning |
|---|---|
| ✓ (green) | Installed |
| ○ (green) | Available locally |
| ○ (yellow) | Fetchable (external, auto-cloned on `add`) |
| ✗ (red) | Not available |

## Data Storage

```
~/.kiro/cc-plugins/
├── config.json      # Registered sources
├── registry.json    # Installed plugin tracking
└── cache/           # Cloned git repos
```

## Development

This is a Cargo workspace:

- `crates/core/` — Core library (source management, scanner, converter, registry)
- `crates/cli/` — CLI binary (`kiro-cc-plugins`)
- `gui/` — Tauri + Svelte desktop app (depends on `crates/core`)

```bash
# Build everything
cargo build --release

# Run the CLI
cargo run -p kiro-cc-plugins -- source list

# Run tests (unit tests)
cargo test

# Run the end-to-end test (requires network, clones real repos)
cargo test -p kiro_cc_core --test e2e -- --ignored

# Build the desktop app
cd gui && npm install && npm run tauri build
```
