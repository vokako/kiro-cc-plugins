<script>
  import { api } from "$lib/api.js";

  let { scope = "global" } = $props();
  let plugins = $state([]);
  let selected = $state(null);
  let detail = $state(null);
  let search = $state("");
  let loading = $state(false);
  let actionLoading = $state(false);
  let error = $state(null);
  let message = $state(null);

  const STATUS_ICON = { installed: "●", available: "○", fetchable: "◌", unavailable: "✗" };
  const STATUS_CLASS = { installed: "st-installed", available: "st-available", fetchable: "st-fetchable", unavailable: "st-unavailable" };
  const TYPE_COLORS = { skill: "#40c0c0", agent: "#8644f0", command: "#f0a030", mcp: "#40c060" };

  let filtered = $derived(
    plugins.filter((p) => !search || p.name.toLowerCase().includes(search.toLowerCase()) || (p.description || "").toLowerCase().includes(search.toLowerCase()))
  );

  let stats = $derived({
    total: plugins.length,
    installed: plugins.filter(p => p.status === "installed").length,
    available: plugins.filter(p => p.status === "available").length,
  });

  export async function load() {
    loading = true;
    error = null;
    try {
      plugins = await api.plugins.list();
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
    if (!confirm(`Delete plugin "${name}"?`)) return;
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
</script>

<section class="layout">
  <!-- LEFT: Plugin list -->
  <div class="list-panel">
    <div class="list-header">
      <h2><span class="hm">▸</span> REGISTRY</h2>
      <div class="stats">
        <span class="stat"><span class="st-installed">●</span> {stats.installed}</span>
        <span class="stat"><span class="st-available">○</span> {stats.available}</span>
        <span class="stat-total">{stats.total}</span>
      </div>
    </div>

    <div class="search-wrap">
      <span class="search-icon">⌕</span>
      <input bind:value={search} placeholder="Filter..." />
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
            <button class="btn-action" onclick={() => update(detail.name)} disabled={actionLoading}>
              ↻ UPDATE
            </button>
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
                <div class="comp-card" style="animation-delay: {i * 40}ms">
                  <div class="comp-type" style="color: {TYPE_COLORS[c.type] || 'var(--text-dim)'}">
                    {c.type}
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

  .stats {
    display: flex;
    gap: 10px;
    font-family: "JetBrains Mono", monospace;
    font-size: 10px;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
  }

  .stat-total {
    color: var(--text-dim);
    background: var(--bg-surface);
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 10px;
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
