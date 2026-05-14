<p align="center">
  <img src="gui/src-tauri/icons/icon.png" width="160" alt="KiroCCPlugins icon" />
</p>

<h1 align="center">kiro-cc-plugins</h1>

<p align="center">
<em>把 <a href="https://code.claude.com/docs/en/plugins">Claude Code 插件</a>安装到 <a href="https://kiro.dev">Kiro</a> 上。</em>
</p>

<p align="center">
  <a href="README.md">English README</a> · Blog: <a href="https://mp.weixin.qq.com/s/4hbZ4E-ZwZQOfLRkiiaJ9Q">让 Kiro 能力倍增，使用 kiro-cc-plugins 一键接入 Claude Code 插件生态</a>
</p>

<p align="center">
  <a href="https://mp.weixin.qq.com/s/4hbZ4E-ZwZQOfLRkiiaJ9Q">
    <img src="docs/blog-qr.png" width="160" alt="扫码阅读公众号文章" />
  </a>
  <br/>
  <sub>微信扫码阅读</sub>
</p>

支持 marketplace 仓库（比如 [Anthropic 官方 marketplace](https://github.com/anthropics/claude-plugins-official)）以及独立的 plugin/skill 仓库。

## 演示

https://github.com/user-attachments/assets/72ae01e3-42fb-4091-8265-ea5346f4f30c

## 安装

### CLI 版

```bash
# 一行安装（macOS / Linux）
curl -fsSL https://raw.githubusercontent.com/vokako/kiro-cc-plugins/main/install.sh | bash

# 或从源码编译
cargo install --git https://github.com/vokako/kiro-cc-plugins kiro-cc-plugins
```

不需要系统级 `git`、Python 或其他运行时——单个自包含的二进制。

### 桌面应用

从 [releases 页面](https://github.com/vokako/kiro-cc-plugins/releases) 下载 `.dmg`（macOS）或 `.exe`（Windows）。

#### macOS 提示"应用已损坏"或"无法打开"

`.dmg` 没有代码签名，所以 macOS Gatekeeper 默认会拦截。把 app 拖到 `/Applications` 后运行一次：

```bash
xattr -d com.apple.quarantine /Applications/KiroCCPlugins.app
```

之后就能正常打开了。或者右键点击 app → **打开** → **打开**（某些 macOS 版本才能用）。

如果你更倾向自己从源码编译，往下看 [架构](#架构) 章节。

## 桌面应用使用指南

### 第一步：添加源

切到 **Sources** 标签，粘贴一个 git 仓库 URL（比如 `https://github.com/anthropics/claude-plugins-official`）。点击 **ADD** 克隆这个 marketplace。你可以添加多个源——每个源卡片显示名称、URL 和最新 commit。**↻ UPDATE ALL** 按钮会拉取所有源的最新版本。

<img width="960" alt="app-sources" src="https://github.com/user-attachments/assets/4fc43c94-562a-44c4-bd18-18c444c0c373" />

### 第二步：浏览和安装插件

点击源卡片会跳到按该源筛选的 **Plugins** 标签，也可以直接切到 Plugins 标签。用搜索栏和源筛选器找插件。点击某个插件查看详情和组件。点击 **⬡ INSTALL** 安装。每个组件卡片有 **ON/OFF** 开关，可以单独启用/禁用 skill、agent 或 MCP server。

<img width="960" alt="app-plugins" src="https://github.com/user-attachments/assets/bdd43418-7a16-4f8b-8e45-aaa8e8e0283b" />

### 第三步：管理已安装的插件

切到 **Installed** 标签查看所有已安装的插件。每个卡片显示组件的启用/禁用状态——点击组件标签可切换。**✓✓** / **✗✗** 批量启用或禁用所有组件。点击卡片跳转到 Plugins 标签的详情页。**✕** 卸载插件。

<img width="960" alt="app-installed" src="https://github.com/user-attachments/assets/ad466897-709b-4fb2-8cc7-397d1f46f031" />

## CLI 快速上手

```bash
# 添加官方 Claude Code marketplace
kiro-cc-plugins source add https://github.com/anthropics/claude-plugins-official

# 添加 Anthropic skills 仓库
kiro-cc-plugins source add https://github.com/anthropics/skills.git

# 查看源和已安装插件的概览
kiro-cc-plugins status

# 按关键词搜索插件
kiro-cc-plugins search chrome

# 浏览所有可用插件
kiro-cc-plugins list

# 查看插件详情
kiro-cc-plugins list feature-dev

# 安装插件（自动检测来源）
kiro-cc-plugins add feature-dev

# 只安装特定类型的组件
kiro-cc-plugins add plugin-dev --only skill

# 直接从任意 git URL 安装
kiro-cc-plugins add --git https://github.com/user/my-cc-plugin

# 更新已安装的插件（git pull + 重新转换）
kiro-cc-plugins update --all

# 卸载插件
kiro-cc-plugins delete feature-dev
```

安装 agent 后，用如下命令运行：

```bash
kiro-cli chat --agent feature-dev
```

### 私有仓库

公开仓库开箱即用。对于私有仓库，凭据按这个顺序解析：`GH_TOKEN`/`GITHUB_TOKEN` 环境变量、git credential helper（osxkeychain/wincred/...）、SSH agent、`~/.ssh/id_*` key。libgit2 处理不了的（FIDO2 key、自定义 SSH 配置、企业级 auth）会兜底到系统 `git` 命令——能在终端 `git clone <url>` 的，工具就能拉。

GitHub 最方便的设置：

```bash
export GH_TOKEN=$(gh auth token)
```

## 命令列表

| 命令 | 说明 |
|---|---|
| `status` | 显示源和已安装插件概览 |
| `source add <url>` | 克隆一个 marketplace 或插件仓库 |
| `source list` | 列出已注册的源 |
| `source update [name]` | Git pull 指定源（或全部） |
| `source remove <name>` | 删除源（如已安装该源插件会拒绝） |
| `source remove <name> --force` | 强制删除源及其插件 |
| `source remove --all --force` | 删除所有源和插件 |
| `search <keyword>` | 跨所有源按名称/描述/分类搜索插件 |
| `list` | 列出所有源的所有插件 |
| `list <plugin>` | 查看插件详情、安装状态、运行命令 |
| `list --installed` | 查看已安装插件及其启用状态 |
| `list --skills` | 列出所有源中的 skill |
| `list --agents` | 列出所有源中的 agent |
| `add <name>` | 安装插件（自动检测源） |
| `add --all` | 安装所有本地可用插件 |
| `add --only skill,agent` | 只安装指定类型的组件 |
| `add --scope workspace` | 安装到 `.kiro/` 而不是 `~/.kiro/` |
| `add --git <url>` | 直接从 git URL 安装 |
| `update <name> / --all` | Git pull 源并重新转换 |
| `update --only skill` | 只重新转换指定类型的组件 |
| `delete <name> / --all` | 卸载插件（别名：`remove`） |
| `delete -y` | 跳过确认提示 |
| `enable <plugin>` | 启用插件的所有组件 |
| `enable <plugin> --component <name>` | 启用特定组件 |
| `enable <plugin> --only skill` | 只启用 skill |
| `disable <plugin>` | 禁用插件的所有组件 |
| `disable <plugin> --component <name>` | 禁用特定组件 |
| `export` | 导出所有配置到 stdout |
| `export -o <file>` | 导出所有配置到文件 |
| `import <file>` | 导入配置（源、插件、启用状态） |

## 转换映射

| Claude Code | Kiro | 转换方式 |
|---|---|---|
| `skills/*/SKILL.md` | `~/.kiro/skills/{source}--{plugin}--{skill}/` | 直接复制 |
| `commands/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` + skill 双重注册 | Prompt → agent JSON + skill |
| `agents/*.md` | `~/.kiro/agents/{source}--{plugin}--{name}.json` | Prompt → agent JSON，工具名映射 |
| `.mcp.json` | `~/.kiro/settings/mcp.json`（键名 `cc-{plugin}-{server}`） | 合并到 Kiro 的 MCP 设置 |
| `hooks/hooks.json` | 每个 agent 的 `hooks` 字段 | 注入到该插件的每个 agent |

## 不支持的 Claude Code 特性

以下 Claude Code 插件特性在 Kiro 里没有对应实现，会在转换时**跳过**：

| 特性 | 使用它的插件 | 跳过原因 |
|---|---|---|
| **LSP servers**（marketplace 中的 `lspServers`） | clangd-lsp、gopls-lsp、pyright-lsp、rust-analyzer-lsp、typescript-lsp 等 | Kiro 不支持自定义 LSP server 配置 |
| **Runtime 代码**（`core/`、`utils/`、`matchers/`、`scripts/`） | hookify 等 | hooks 依赖的 Python/shell 运行时 |
| **`strict` 模式** | 少数 marketplace 条目 | Kiro 没有对应的强制机制 |

Skills、agents、commands、MCP servers 和 command 类型的 hooks 都完整转换。

## 启用 / 禁用

组件可以单独启用或禁用，不用卸载整个插件：

- **Skills**：禁用时移到 `~/.kiro/cc-plugins/disabled-skills/`
- **Agents/Commands**：禁用时移到 `~/.kiro/cc-plugins/disabled-agents/`
- **MCP servers**：在 `~/.kiro/settings/mcp.json` 里设 `"disabled": true`

桌面应用里每个组件卡片有切换按钮。

## 状态图标

| 图标 | 含义 |
|---|---|
| ✓（绿色） | 已安装且启用 |
| ✗ | 已禁用 |
| ○（绿色） | 本地可用 |
| ○（黄色） | 可拉取（外部仓库，`add` 时自动克隆） |

## 数据存储

```
~/.kiro/cc-plugins/
├── config.json          # 注册的源
├── registry.json        # 已安装插件追踪
├── cache/               # 克隆的 git 仓库
├── disabled-skills/     # 停用的 skill 存放处
└── disabled-agents/     # 停用的 agent 存放处
```

## 架构

Cargo workspace，三个 crate：

- `crates/core/` — 核心库（source、scanner、converter、registry、JSON API）
- `crates/cli/` — CLI 二进制（`kiro-cc-plugins`）
- `gui/` — Tauri + Svelte 桌面应用（把 `crates/core` 作为进程内 SDK 使用）

```bash
# 编译全部
cargo build --release

# 运行 CLI
cargo run -p kiro-cc-plugins -- status

# 运行测试
cargo test
cargo test -p kiro_cc_core --test e2e -- --ignored  # 需要联网

# 编译桌面应用
cd gui && npm install && npm run tauri build
```
