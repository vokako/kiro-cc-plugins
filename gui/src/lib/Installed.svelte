<script>
  import { api } from "$lib/api.js";
  import { confirm as tauriConfirm } from "@tauri-apps/plugin-dialog";

  let { scope = "global", openPlugin } = $props();
  let installed = $state([]);
  let loading = $state(false);
  let actionLoading = $state(null);
  let error = $state(null);
  let message = $state(null);

  export async function load() {
    loading = true;
    error = null;
    try {
      installed = await api.plugins.list({ installed: true });
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function update(name) {
    actionLoading = name;
    message = null;
    error = null;
    try {
      await api.plugins.update(name);
      message = `${name} updated`;
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = null;
    }
  }

  async function remove(name) {
    if (!(await tauriConfirm(`Delete plugin "${name}"?`, { title: "Confirm Delete", kind: "warning" }))) return;
    actionLoading = name;
    message = null;
    error = null;
    try {
      await api.plugins.delete(name);
      message = `${name} removed`;
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = null;
    }
  }

  async function updateAll() {
    actionLoading = "__all__";
    message = null;
    error = null;
    try {
      await api.plugins.update(null);
      message = "All plugins updated";
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = null;
    }
  }

  async function toggleComponent(pluginName, componentName, enable) {
    actionLoading = pluginName;
    error = null;
    try {
      await api.plugins.toggle(pluginName, { component: componentName, enable });
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = null;
    }
  }

  async function toggleAll(pluginName, enable) {
    actionLoading = pluginName;
    error = null;
    try {
      await api.plugins.toggle(pluginName, { enable });
      message = `${pluginName} ${enable ? "enabled" : "disabled"} (all components)`;
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = null;
    }
  }

  load();
</script>

<section>
  <div class="section-header">
    <h2><span class="hm">▸</span> INSTALLED <span class="count">{installed.length}</span></h2>
  </div>

  {#if error}
    <div class="toast error"><span>✗</span> {error}</div>
  {/if}
  {#if message}
    <div class="toast success"><span>✓</span> {message}</div>
  {/if}

  {#if loading && installed.length === 0}
    <div class="empty"><span class="spinner"></span></div>
  {:else if installed.length === 0}
    <div class="empty">
      <span class="empty-glyph">◇</span>
      <p>No plugins installed</p>
      <p class="hint">Go to Plugins tab to browse and install</p>
    </div>
  {:else}
    <div class="grid">
      {#each installed as p, i}
        <div
          class="card"
          style="animation-delay: {i * 40}ms"
          role="button"
          tabindex="0"
          onclick={() => openPlugin?.(p.plugin_name)}
          onkeydown={(e) => (e.key === "Enter" || e.key === " ") && openPlugin?.(p.plugin_name)}
        >
          <div class="card-top">
            <div class="card-title">
              <span class="dot"></span>
              <span class="name">{p.plugin_name}</span>
            </div>
            <span class="source-tag">{p.source_name}</span>
          </div>

          <div class="comp-list">
            {#each p.components || [] as c}
              <button
                class="comp"
                class:comp-skill={c.type === "skill"}
                class:comp-agent={c.type === "agent" || c.type === "command"}
                class:comp-mcp={c.type === "mcp"}
                class:comp-disabled={c.enabled === false}
                onclick={(e) => { e.stopPropagation(); toggleComponent(p.plugin_name, c.name, c.enabled === false); }}
                disabled={actionLoading}
                title={c.enabled === false ? "Click to enable" : "Click to disable"}
              >
                {c.enabled === false ? "✗" : "✓"} {c.type}:{c.name}
              </button>
            {/each}
          </div>

          <div class="card-footer">
            <span class="date">{(p.installed_at || "").slice(0, 10)}</span>
            <span class="scope-badge">{p.scope || "global"}</span>
            <div class="card-actions">
              <button class="btn-sm" onclick={(e) => { e.stopPropagation(); toggleAll(p.plugin_name, true); }} disabled={actionLoading} title="Enable all">✓✓</button>
              <button class="btn-sm" onclick={(e) => { e.stopPropagation(); toggleAll(p.plugin_name, false); }} disabled={actionLoading} title="Disable all">✗✗</button>
              <button class="btn-sm btn-del" onclick={(e) => { e.stopPropagation(); remove(p.plugin_name); }} disabled={actionLoading}>✕</button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  section {
    padding: 20px 24px;
    height: 100%;
    overflow-y: auto;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }

  h2 {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.2em;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hm { color: var(--amber); }

  .count {
    font-size: 10px;
    background: var(--bg-surface);
    padding: 1px 7px;
    border-radius: 8px;
    color: var(--amber);
    border: 1px solid var(--border);
  }

  .header-actions {
    display: flex;
    gap: 6px;
  }

  .btn-ghost {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.08em;
    background: transparent;
    border: 1px solid var(--border);
    padding: 5px 12px;
    color: var(--text-dim);
  }

  .btn-ghost:hover {
    color: var(--text);
    border-color: var(--border-bright);
    background: var(--bg-raised);
  }

  .toast {
    padding: 8px 12px;
    border-radius: 3px;
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 16px;
    animation: slideIn 0.2s ease-out;
  }

  .toast.error {
    background: rgba(224, 64, 64, 0.08);
    border: 1px solid rgba(224, 64, 64, 0.2);
    color: var(--red);
  }

  .toast.success {
    background: rgba(64, 192, 96, 0.08);
    border: 1px solid rgba(64, 192, 96, 0.2);
    color: var(--green);
  }

  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-6px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 10px;
  }

  .card {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 14px 16px;
    cursor: pointer;
    transition: border-color 0.2s;
    animation: fadeUp 0.25s ease-out both;
  }

  .card:hover {
    border-color: var(--border-bright);
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 5px var(--green-dim);
  }

  .name {
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 13px;
    color: var(--text-bright);
  }

  .source-tag {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    color: var(--text-dim);
    background: var(--bg-surface);
    padding: 2px 7px;
    border-radius: 2px;
    letter-spacing: 0.04em;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .comp-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 12px;
  }

  .comp {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 3px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
    cursor: pointer;
  }

  .comp:hover {
    background: var(--bg-raised);
    border-color: var(--border-bright);
  }

  .comp-disabled {
    opacity: 0.4;
    text-decoration: line-through;
  }

  .comp-skill { color: var(--cyan); border-color: rgba(64, 192, 192, 0.2); }
  .comp-agent { color: var(--amber); border-color: rgba(134, 68, 240, 0.2); }
  .comp-mcp { color: var(--green); border-color: rgba(64, 192, 96, 0.2); }

  .card-footer {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .date {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--text-dim);
    opacity: 0.6;
  }

  .scope-badge {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    background: var(--bg);
    padding: 1px 6px;
    border-radius: 2px;
    border: 1px solid var(--border);
  }

  .card-actions {
    margin-left: auto;
    display: flex;
    gap: 4px;
  }

  .btn-sm {
    background: transparent;
    border: 1px solid transparent;
    padding: 3px 8px;
    font-size: 13px;
    color: var(--text-dim);
    border-radius: 3px;
  }

  .btn-sm:hover {
    background: var(--bg-surface);
    border-color: var(--border);
    color: var(--text);
  }

  .btn-del:hover {
    color: var(--red);
    border-color: rgba(224, 64, 64, 0.3);
    background: rgba(224, 64, 64, 0.08);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    color: var(--text-dim);
    gap: 8px;
  }

  .empty-glyph {
    font-size: 32px;
    color: var(--border-bright);
    margin-bottom: 8px;
  }

  .hint { font-size: 12px; opacity: 0.6; }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid var(--border);
    border-top-color: var(--amber);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin { to { transform: rotate(360deg); } }
</style>
