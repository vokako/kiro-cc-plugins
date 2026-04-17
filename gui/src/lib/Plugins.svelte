<script>
  import { api } from "$lib/api.js";
  import { confirm as tauriConfirm } from "@tauri-apps/plugin-dialog";

  let { scope = "global", jumpTo = $bindable(null), initialSource = $bindable(null) } = $props();
  let plugins = $state([]);
  let selected = $state(null);
  let detail = $state(null);
  let search = $state("");
  let statusFilter = $state("all"); // all | installed | available
  let sourceFilter = $state("");
  let sources = $state([]);
  let loading = $state(false);
  let actionLoading = $state(false);
  let error = $state(null);
  let message = $state(null);

  const STATUS_ICON = { installed: "●", available: "○", fetchable: "◌", unavailable: "✗" };
  const STATUS_CLASS = { installed: "st-installed", available: "st-available", fetchable: "st-fetchable", unavailable: "st-unavailable" };
  const TYPE_COLORS = { skill: "#40c0c0", agent: "#8644f0", command: "#f0a030", mcp: "#40c060" };

  let filtered = $derived(
    plugins.filter((p) => {
      if (search && !p.name.toLowerCase().includes(search.toLowerCase()) && !(p.description || "").toLowerCase().includes(search.toLowerCase())) return false;
      if (statusFilter === "installed" && p.status !== "installed") return false;
      if (statusFilter === "available" && p.status !== "available" && p.status !== "fetchable") return false;
      if (sourceFilter && p.source_name !== sourceFilter) return false;
      return true;
    })
  );

  let stats = $derived({
    total: plugins.length,
    installed: plugins.filter(p => p.status === "installed").length,
    showing: filtered.length,
  });

  export function getStats() { return stats; }

  export async function load() {
    loading = true;
    error = null;
    try {
      plugins = await api.plugins.list();
      sources = await api.sources.list();
      if (initialSource) {
        sourceFilter = initialSource;
        initialSource = null;
      }
      if (jumpTo) {
        const name = jumpTo;
        jumpTo = null;
        await showDetail(name);
      }
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function showDetail(name) {
    selected = name;
    detail = null;
    try {
      detail = await api.plugins.detail(name);
    } catch (e) {
      error = e.message;
    }
  }

  async function install(name) {
    actionLoading = true;
    message = null;
    error = null;
    try {
      const results = await api.plugins.add(name, { scope });
      const r = results?.[0];
      message = r ? `${r.name} → ${r.components} component(s) installed` : "Done";
      await load();
      if (selected) await showDetail(selected);
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = false;
    }
  }

  async function remove(name) {
    if (!(await tauriConfirm(`Delete plugin "${name}"?`, { title: "Confirm Delete", kind: "warning" }))) return;
    actionLoading = true;
    message = null;
    error = null;
    try {
      await api.plugins.delete(name);
      message = `${name} removed`;
      await load();
      if (selected === name) { selected = null; detail = null; }
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = false;
    }
  }

  async function update(name) {
    actionLoading = true;
    message = null;
    error = null;
    try {
      await api.plugins.update(name);
      message = `${name} updated`;
      await load();
      if (selected) await showDetail(selected);
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = false;
    }
  }

  load();

  async function toggleComponent(pluginName, componentName, enable) {
    actionLoading = true;
    error = null;
    try {
      await api.plugins.toggle(pluginName, { component: componentName, enable });
      if (selected) await showDetail(selected);
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = false;
    }
  }

  async function toggleAll(pluginName, enable) {
    actionLoading = true;
    error = null;
    try {
      await api.plugins.toggle(pluginName, { enable });
      if (selected) await showDetail(selected);
    } catch (e) {
      error = e.message;
    } finally {
      actionLoading = false;
    }
  }
</script>

<section class="layout">
  <!-- LEFT: Plugin list -->
  <div class="list-panel">
    <div class="list-header">
      <h2><span class="hm">▸</span> REGISTRY</h2>
      <label class="toggle">
        <span class="toggle-label">{statusFilter === "installed" ? "Installed" : "All"}</span>
        <input type="checkbox" checked={statusFilter === "installed"} onchange={(e) => statusFilter = e.target.checked ? "installed" : "all"} />
        <span class="toggle-track">
          <span class="toggle-thumb"></span>
        </span>
      </label>
    </div>

    <div class="search-wrap">
      <span class="search-icon">⌕</span>
      <input bind:value={search} placeholder="Filter..." />
    </div>

    <div class="filters">
      <select bind:value={sourceFilter} class="filter-select">
        <option value="">All sources</option>
        {#each sources as s}
          <option value={s.name}>{s.name}</option>
        {/each}
      </select>
    </div>

    {#if error}
      <div class="toast error"><span>✗</span> {error}</div>
    {/if}
    {#if message}
      <div class="toast success"><span>✓</span> {message}</div>
    {/if}

    {#if loading && plugins.length === 0}
      <div class="empty"><span class="spinner"></span></div>
    {:else if filtered.length === 0}
      <div class="empty">
        <span class="empty-glyph">◇</span>
        <span>{plugins.length === 0 ? "No plugins. Add a source." : "No matches."}</span>
      </div>
    {:else}
      <div class="plugin-list">
        {#each filtered as p, i}
          <button
            class="plugin-row"
            class:active={selected === p.name}
            onclick={() => showDetail(p.name)}
            style="animation-delay: {Math.min(i * 20, 400)}ms"
          >
            <span class="row-status {STATUS_CLASS[p.status]}">{STATUS_ICON[p.status]}</span>
            <div class="row-text">
              <span class="row-name">{p.name}</span>
              <span class="row-desc">{p.description || ""}</span>
            </div>
            {#if p.category}
              <span class="row-cat">{p.category}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- RIGHT: Detail panel -->
  <div class="detail-panel">
    {#if detail}
      <div class="detail-content" style="animation: fadeIn 0.25s ease-out">
        <div class="detail-top">
          <div>
            <h3>{detail.name}</h3>
            <p class="detail-desc">{detail.description}</p>
          </div>
          <div class="detail-badge" class:installed={detail.installed}>
            {detail.installed ? "INSTALLED" : "AVAILABLE"}
          </div>
        </div>

        <div class="detail-meta">
          {#if detail.category}
            <span class="meta-tag">{detail.category}</span>
          {/if}
          <span class="meta-source">
            <span class="meta-label">SRC</span> {detail.source}
          </span>
        </div>

        <div class="detail-actions">
          {#if detail.installed}
            <button class="btn-action" onclick={() => toggleAll(detail.name, true)} disabled={actionLoading}>✓ ENABLE ALL</button>
            <button class="btn-action" onclick={() => toggleAll(detail.name, false)} disabled={actionLoading}>✗ DISABLE ALL</button>
            <button class="btn-action btn-del" onclick={() => remove(detail.name)} disabled={actionLoading}>
              ✕ DELETE
            </button>
          {:else}
            <button class="btn-install" onclick={() => install(detail.name)} disabled={actionLoading}>
              {actionLoading ? "..." : "⬡ INSTALL"}
            </button>
          {/if}
        </div>

        {#if detail.components?.length}
          <div class="comp-section">
            <h4><span class="hm">▸</span> COMPONENTS <span class="comp-count">{detail.components.length}</span></h4>
            <div class="comp-grid">
              {#each detail.components as c, i}
                <div class="comp-card" class:comp-off={c.enabled === false} style="animation-delay: {i * 40}ms">
                  <div class="comp-card-header">
                    <div class="comp-type" style="color: {TYPE_COLORS[c.type] || 'var(--text-dim)'}">
                      {c.type}
                    </div>
                    {#if c.enabled !== undefined}
                      <button
                        class="comp-toggle"
                        class:comp-toggle-on={c.enabled !== false}
                        onclick={() => toggleComponent(detail.name, c.name, c.enabled === false)}
                        disabled={actionLoading}
                        title={c.enabled === false ? "Enable" : "Disable"}
                      >{c.enabled === false ? "OFF" : "ON"}</button>
                    {/if}
                  </div>
                  <div class="comp-name">{c.name}</div>
                  {#if c.description}
                    <div class="comp-desc">{c.description}</div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {:else if selected}
      <div class="empty"><span class="spinner"></span></div>
    {:else}
      <div class="empty-detail">
        <div class="empty-graphic">
          <span>⬡</span>
        </div>
        <p>Select a plugin to inspect</p>
      </div>
    {/if}
  </div>
</section>

<style>
  .layout {
    display: flex;
    height: 100%;
  }

  /* ── List Panel ── */
  .list-panel {
    width: 340px;
    min-width: 280px;
    background: var(--bg);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .list-header {
    padding: 14px 16px 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  h2 {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.2em;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .hm { color: var(--amber); }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    -webkit-app-region: no-drag;
  }

  .toggle input { display: none; }

  .toggle-track {
    width: 28px;
    height: 14px;
    background: var(--border);
    border-radius: 7px;
    position: relative;
    transition: background 0.2s;
  }

  .toggle input:checked + .toggle-track {
    background: var(--amber);
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 10px;
    height: 10px;
    background: var(--text-bright);
    border-radius: 50%;
    transition: transform 0.2s;
  }

  .toggle input:checked + .toggle-track .toggle-thumb {
    transform: translateX(14px);
  }

  .toggle-label {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--text-dim);
    letter-spacing: 0.05em;
  }

  .search-wrap {
    margin: 0 12px 8px;
    display: flex;
    align-items: center;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 3px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }

  .filters {
    display: flex;
    gap: 6px;
    margin: 0 12px 10px;
  }

  .filter-select {
    flex: 1;
    font-size: 11px;
    padding: 5px 8px;
    min-width: 0;
    background: var(--bg-raised);
  }

  .search-wrap:focus-within {
    border-color: var(--amber);
    box-shadow: 0 0 0 2px var(--amber-glow);
  }

  .search-icon {
    padding: 0 0 0 10px;
    color: var(--text-dim);
    font-size: 14px;
  }

  .search-wrap input {
    border: none;
    background: transparent;
    flex: 1;
    padding: 7px 10px 7px 6px;
    font-size: 12px;
  }

  .search-wrap input:focus {
    box-shadow: none;
  }

  .toast {
    margin: 0 12px 8px;
    padding: 8px 12px;
    border-radius: 3px;
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    display: flex;
    align-items: center;
    gap: 6px;
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

  .plugin-list {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 8px;
  }

  .plugin-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 10px;
    border: 1px solid transparent;
    background: transparent;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
    width: 100%;
    color: inherit;
    font-size: inherit;
    font-family: inherit;
    transition: all 0.12s;
    animation: fadeUp 0.25s ease-out both;
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .plugin-row:hover {
    background: var(--bg-raised);
    border-color: var(--border);
  }

  .plugin-row.active {
    background: var(--bg-surface);
    border-color: var(--amber-dim);
    box-shadow: inset 3px 0 0 var(--amber);
  }

  .row-status {
    flex-shrink: 0;
    margin-top: 3px;
    font-size: 10px;
  }

  .st-installed { color: var(--green); }
  .st-available { color: var(--green); opacity: 0.6; }
  .st-fetchable { color: var(--amber); }
  .st-unavailable { color: var(--red); opacity: 0.5; }

  .row-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row-name {
    font-family: "JetBrains Mono", monospace;
    font-weight: 500;
    font-size: 12px;
    color: var(--text-bright);
  }

  .row-desc {
    font-size: 11px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-cat {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    color: var(--text-dim);
    background: var(--bg-surface);
    padding: 2px 6px;
    border-radius: 2px;
    flex-shrink: 0;
    margin-top: 2px;
    letter-spacing: 0.05em;
  }

  /* ── Detail Panel ── */
  .detail-panel {
    flex: 1;
    background: var(--bg-deep);
    overflow-y: auto;
    position: relative;
  }

  .detail-content {
    padding: 24px 28px;
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .detail-top {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 16px;
  }

  h3 {
    font-family: "JetBrains Mono", monospace;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-bright);
    margin-bottom: 6px;
    letter-spacing: -0.01em;
  }

  .detail-desc {
    font-size: 13px;
    color: var(--text-dim);
    line-height: 1.6;
  }

  .detail-badge {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.15em;
    padding: 4px 10px;
    border-radius: 3px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
  }

  .detail-badge.installed {
    color: var(--green);
    border-color: var(--green-dim);
    background: rgba(64, 192, 96, 0.06);
  }

  .detail-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 20px;
  }

  .meta-tag {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    padding: 3px 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--cyan);
    letter-spacing: 0.05em;
  }

  .meta-source {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    color: var(--text-dim);
  }

  .meta-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.1em;
    color: var(--text-dim);
    opacity: 0.6;
    margin-right: 4px;
  }

  .detail-actions {
    display: flex;
    gap: 8px;
    margin-bottom: 28px;
  }

  .btn-install {
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.1em;
    background: var(--amber);
    color: var(--bg-deep);
    border: none;
    padding: 9px 24px;
  }

  .btn-install:hover {
    background: #9a60f8;
    color: var(--bg-deep);
    box-shadow: 0 0 16px var(--amber-glow);
  }

  .btn-action {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.08em;
    padding: 8px 16px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
  }

  .btn-action:hover {
    border-color: var(--border-bright);
  }

  .btn-del:hover {
    color: var(--red);
    border-color: rgba(224, 64, 64, 0.4);
    background: rgba(224, 64, 64, 0.06);
  }

  .comp-section {
    border-top: 1px solid var(--border);
    padding-top: 20px;
  }

  h4 {
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.2em;
    color: var(--text-dim);
    margin-bottom: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .comp-count {
    font-size: 10px;
    background: var(--bg-surface);
    padding: 1px 6px;
    border-radius: 8px;
    color: var(--amber);
    border: 1px solid var(--border);
  }

  .comp-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }

  .comp-card {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 12px 14px;
    transition: border-color 0.15s;
    animation: fadeUp 0.25s ease-out both;
  }

  .comp-card:hover {
    border-color: var(--border-bright);
  }

  .comp-off {
    opacity: 0.4;
  }

  .comp-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .comp-toggle {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.1em;
    padding: 2px 6px;
    border: 1px solid var(--border);
    background: var(--bg-surface);
    color: var(--text-dim);
    border-radius: 2px;
  }

  .comp-toggle-on {
    color: var(--green);
    border-color: var(--green-dim);
  }

  .comp-toggle:hover {
    border-color: var(--border-bright);
  }

  .comp-type {
    font-family: "JetBrains Mono", monospace;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    margin-bottom: 4px;
  }

  .comp-name {
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 12px;
    color: var(--text-bright);
    margin-bottom: 4px;
  }

  .comp-desc {
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  /* ── Empty states ── */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 40px;
    color: var(--text-dim);
    gap: 8px;
    font-size: 12px;
  }

  .empty-glyph {
    font-size: 24px;
    color: var(--border-bright);
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid var(--border);
    border-top-color: var(--amber);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .empty-detail {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-dim);
    gap: 16px;
    font-size: 12px;
  }

  .empty-graphic span {
    font-size: 48px;
    color: var(--border);
    opacity: 0.4;
  }
</style>
