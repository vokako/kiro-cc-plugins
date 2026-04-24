<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api.js";
  import { appUpdate, sourceUpdates, checkUpdates } from "$lib/updates.js";
  import Sources from "$lib/Sources.svelte";
  import Plugins from "$lib/Plugins.svelte";
  import Installed from "$lib/Installed.svelte";

  let tab = $state("plugins");
  let version = $state("");
  let pluginsRef = $state(null);
  let installedRef = $state(null);
  let jumpTo = $state(null);
  let initialSource = $state(null);
  let toast = $state(null); // { kind: 'ok'|'err', text: string }
  let toastTimer = null;

  function showToast(kind, text) {
    if (toastTimer) clearTimeout(toastTimer);
    toast = { kind, text };
    toastTimer = setTimeout(() => { toast = null; toastTimer = null; }, 4000);
  }

  function onSourceChange() {
    pluginsRef?.load();
  }

  function refresh() {
    if (tab === "plugins") pluginsRef?.load();
    else if (tab === "installed") installedRef?.load();
  }

  function openPlugin(name) {
    jumpTo = name;
    tab = "plugins";
  }

  function openSource(sourceName) {
    initialSource = sourceName;
    tab = "plugins";
  }

  async function exportConfig() {
    try {
      const data = await api.config.export();
      const path = await save({ defaultPath: "kiro-cc-plugins.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      await writeTextFile(path, JSON.stringify(data, null, 2));
      const ns = data?.sources?.length ?? 0;
      const np = data?.plugins?.length ?? 0;
      showToast("ok", `Exported ${ns} sources, ${np} plugins`);
    } catch (e) {
      console.error("export failed:", e);
      showToast("err", `Export failed: ${e.message || e}`);
    }
  }

  async function importConfig() {
    try {
      const path = await open({ filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const text = await readTextFile(path);
      const config = JSON.parse(text);
      if (!config || typeof config !== "object" || config.version !== 1) {
        showToast("err", "Invalid config file (missing version: 1)");
        return;
      }
      showToast("ok", "Importing sources and plugins...");
      const result = await api.config.import(config);
      const ns = result?.sources?.length ?? 0;
      const np = result?.plugins?.length ?? 0;
      const sFails = (result?.sources || []).filter(s => s.status !== "ok").length;
      const pFails = (result?.plugins || []).filter(p => p.status !== "ok").length;
      if (sFails || pFails) {
        showToast("err", `Imported with errors: ${ns - sFails}/${ns} sources, ${np - pFails}/${np} plugins`);
      } else {
        showToast("ok", `Imported ${ns} sources, ${np} plugins`);
      }
      pluginsRef?.load();
      installedRef?.load();
    } catch (e) {
      console.error("import failed:", e);
      showToast("err", `Import failed: ${e.message || e}`);
    }
  }

  let zoomLevel = 1;

  onMount(async () => {
    try { version = await invoke("app_version"); } catch (_) {}
    // Initial update check
    if (version) checkUpdates(version);
    // Re-check when window regains focus (rate-limited internally)
    window.addEventListener("focus", () => {
      if (version) checkUpdates(version);
    });
    window.addEventListener("keydown", (e) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.key === "=" || e.key === "+") { e.preventDefault(); zoomLevel = Math.min(2, zoomLevel + 0.1); document.body.style.zoom = zoomLevel; }
      else if (e.key === "-") { e.preventDefault(); zoomLevel = Math.max(0.5, zoomLevel - 0.1); document.body.style.zoom = zoomLevel; }
      else if (e.key === "0") { e.preventDefault(); zoomLevel = 1; document.body.style.zoom = 1; }
    });
  });

  async function handleAppUpdate() {
    const url = $appUpdate?.download_url || $appUpdate?.release_url;
    if (!url) return;
    try {
      await openUrl(url);
    } catch (e) {
      showToast("err", `Failed to open: ${e.message || e}`);
    }
  }
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="logo">⬡</span>
      <span class="title">KIRO <span class="accent">CC PLUGINS</span></span>
      {#if version}
        <span class="version">
          v{version}
          {#if $appUpdate?.has_update}
            <button class="update-badge" onclick={handleAppUpdate} title="New version v{$appUpdate.latest} available — click to download">
              NEW
            </button>
          {/if}
        </span>
      {/if}
    </div>
    <nav>
      <button class:active={tab === "plugins"} onclick={() => (tab = "plugins")}>
        <span class="nav-icon">◈</span> Plugins
      </button>
      <button class:active={tab === "installed"} onclick={() => (tab = "installed")}>
        <span class="nav-icon">●</span> Installed
      </button>
      <button class:active={tab === "sources"} onclick={() => (tab = "sources")}>
        <span class="nav-icon">◇</span> Sources
      </button>
    </nav>
    <div class="header-right">
      <button class="header-btn" onclick={importConfig} title="Load config from file">LOAD</button>
      <button class="header-btn" onclick={exportConfig} title="Save config to file">SAVE</button>
      <button class="header-btn" onclick={refresh} title="Refresh">↻</button>
    </div>
  </header>

  <main>
    {#if tab === "plugins"}
      <Plugins bind:this={pluginsRef} bind:jumpTo bind:initialSource />
    {:else if tab === "installed"}
      <Installed bind:this={installedRef} {openPlugin} />
    {:else}
      <Sources onsourceChange={onSourceChange} {openSource} />
    {/if}
  </main>

  {#if toast}
    <div class="toast toast-{toast.kind}">{toast.text}</div>
  {/if}

  <footer>
    <span class="status-dot"></span>
    <span>READY</span>
    {#if tab === "plugins" && pluginsRef}
      {@const s = pluginsRef.getStats?.() || {}}
      {#if s.total}
        <span class="footer-sep">│</span>
        <span>{s.showing}/{s.total} plugins</span>
        <span class="footer-sep">│</span>
        <span>{s.installed} installed</span>
      {/if}
    {/if}
  </footer>
</div>

<style>
  :global(*) {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  :global(:root) {
    --amber: #8644f0;
    --amber-dim: #6030b0;
    --amber-glow: #8644f040;
    --green: #40c060;
    --green-dim: #2a8040;
    --red: #e04040;
    --red-dim: #a02020;
    --cyan: #40c0c0;

    --bg-deep: #0a0a0c;
    --bg: #101014;
    --bg-raised: #16161c;
    --bg-surface: #1c1c24;
    --border: #2a2a35;
    --border-bright: #3a3a48;
    --text: #d0d0d8;
    --text-dim: #606070;
    --text-bright: #f0f0f5;

    font-family: "DM Sans", sans-serif;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text);
    background: var(--bg-deep);
    -webkit-font-smoothing: antialiased;
  }

  :global(input, select, button) {
    font-family: "JetBrains Mono", monospace;
    font-size: 12px;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 7px 12px;
    outline: none;
    transition: border-color 0.15s, box-shadow 0.15s, background 0.15s;
  }

  :global(input:focus, select:focus) {
    border-color: var(--amber);
    box-shadow: 0 0 0 2px var(--amber-glow);
  }

  :global(input::placeholder) {
    color: var(--text-dim);
    font-style: italic;
  }

  :global(button) {
    cursor: pointer;
    letter-spacing: 0.02em;
  }

  :global(button:hover) {
    background: var(--bg-raised);
    border-color: var(--border-bright);
  }

  :global(button:active) {
    transform: translateY(1px);
  }

  :global(button:disabled) {
    opacity: 0.35;
    cursor: not-allowed;
    transform: none;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background:
      repeating-linear-gradient(
        0deg,
        transparent,
        transparent 2px,
        rgba(255, 255, 255, 0.008) 2px,
        rgba(255, 255, 255, 0.008) 4px
      ),
      var(--bg-deep);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 20px;
    height: 48px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    -webkit-app-region: drag;
    position: relative;
  }

  header::after {
    content: "";
    position: absolute;
    bottom: -1px;
    left: 0;
    right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--amber-dim), transparent);
    opacity: 0.4;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    -webkit-app-region: no-drag;
  }

  .logo {
    font-size: 18px;
    color: var(--amber);
    filter: drop-shadow(0 0 4px var(--amber-glow));
    animation: pulse 3s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }

  .title {
    font-family: "JetBrains Mono", monospace;
    font-weight: 700;
    font-size: 14px;
    letter-spacing: 0.15em;
    color: var(--text-bright);
  }

  .accent {
    color: var(--amber);
  }

  .version {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    color: var(--text-dim);
    background: var(--bg-raised);
    padding: 2px 6px;
    border-radius: 2px;
    letter-spacing: 0.05em;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .update-badge {
    font-family: "JetBrains Mono", monospace;
    font-size: 8px;
    font-weight: 700;
    color: var(--bg);
    background: var(--amber);
    padding: 1px 5px;
    border-radius: 2px;
    letter-spacing: 0.08em;
    border: none;
    cursor: pointer;
    -webkit-app-region: no-drag;
    animation: pulse 2s ease-in-out infinite;
  }
  .update-badge:hover {
    filter: brightness(1.15);
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.75; }
  }

  nav {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 2px;
    -webkit-app-region: no-drag;
  }

  nav button {
    border: 1px solid transparent;
    background: transparent;
    padding: 6px 18px;
    border-radius: 3px;
    font-family: "DM Sans", sans-serif;
    font-weight: 500;
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
    transition: all 0.2s;
  }

  nav button:hover {
    color: var(--text);
    background: var(--bg-raised);
  }

  nav button.active {
    color: var(--amber);
    background: var(--bg-surface);
    border-color: var(--border);
    box-shadow: inset 0 -2px 0 var(--amber);
  }

  .nav-icon {
    font-size: 10px;
    margin-right: 4px;
    opacity: 0.7;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 12px;
    -webkit-app-region: no-drag;
  }

  .header-btn {
    background: transparent;
    border: 1px solid var(--border);
    padding: 4px 10px;
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--text-dim);
    border-radius: 3px;
    transition: all 0.2s;
  }

  .header-btn:hover {
    color: var(--amber);
    border-color: var(--amber);
    background: rgba(134, 68, 240, 0.06);
  }

  .header-btn:active {
    transform: translateY(1px);
  }

  .toast {
    position: fixed;
    bottom: 36px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 16px;
    border-radius: 4px;
    font-size: 12px;
    font-family: "JetBrains Mono", monospace;
    letter-spacing: 0.03em;
    z-index: 1000;
    animation: toast-in 0.2s ease-out;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }
  .toast-ok {
    background: var(--green);
    color: var(--bg);
  }
  .toast-err {
    background: var(--red);
    color: var(--text-bright);
  }
  @keyframes toast-in {
    from { opacity: 0; transform: translate(-50%, 10px); }
    to { opacity: 1; transform: translate(-50%, 0); }
  }

  main {
    flex: 1;
    overflow: hidden;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 20px;
    height: 24px;
    background: var(--bg);
    border-top: 1px solid var(--border);
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--text-dim);
    letter-spacing: 0.08em;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 6px var(--green-dim);
    animation: pulse 2s ease-in-out infinite;
  }

  .footer-sep {
    opacity: 0.3;
    margin: 0 4px;
  }
</style>
