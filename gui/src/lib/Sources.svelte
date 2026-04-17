<script>
  import { api } from "$lib/api.js";
  import { confirm as tauriConfirm } from "@tauri-apps/plugin-dialog";

  let { onsourceChange, openSource } = $props();
  let sources = $state([]);
  let newUrl = $state("");
  let loading = $state(false);
  let error = $state(null);
  let message = $state(null);

  async function load() {
    loading = true;
    error = null;
    try {
      sources = await api.sources.list();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function addSource() {
    if (!newUrl.trim()) return;
    loading = true;
    error = null;
    message = null;
    try {
      await api.sources.add(newUrl.trim());
      message = "Source added";
      newUrl = "";
      await load();
      onsourceChange?.();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function removeSource(name) {
    if (!(await tauriConfirm(`Remove source "${name}"?`, { title: "Confirm Remove", kind: "warning" }))) return;
    loading = true;
    error = null;
    message = null;
    try {
      await api.sources.remove(name);
      await load();
      onsourceChange?.();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function updateSource(name) {
    loading = true;
    error = null;
    message = null;
    try {
      await api.sources.update(name);
      message = `${name} updated`;
      await load();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  load();
</script>

<section>
  <div class="section-header">
    <h2>
      <span class="header-marker">▸</span>
      GIT SOURCES
      <span class="count">{sources.length}</span>
    </h2>
  </div>

  <div class="add-row">
    <div class="input-wrap">
      <span class="input-prefix">$</span>
      <input
        bind:value={newUrl}
        placeholder="git clone <url>"
        onkeydown={(e) => e.key === "Enter" && addSource()}
        disabled={loading}
      />
    </div>
    <button class="btn-primary" onclick={addSource} disabled={loading || !newUrl.trim()}>
      {loading ? "..." : "ADD"}
    </button>
  </div>

  {#if error}
    <div class="toast error">
      <span class="toast-icon">✗</span> {error}
    </div>
  {/if}
  {#if message}
    <div class="toast success">
      <span class="toast-icon">✓</span> {message}
    </div>
  {/if}

  {#if loading && sources.length === 0}
    <div class="empty">
      <span class="spinner"></span> Fetching sources...
    </div>
  {:else if sources.length === 0}
    <div class="empty">
      <span class="empty-icon">◇</span>
      <p>No sources registered</p>
      <p class="hint">Add a marketplace or plugin git URL above</p>
    </div>
  {:else}
    <div class="source-grid">
      {#each sources as s, i}
        <div
          class="source-card"
          style="animation-delay: {i * 50}ms"
          role="button"
          tabindex="0"
          onclick={() => openSource?.(s.name)}
          onkeydown={(e) => (e.key === "Enter" || e.key === " ") && openSource?.(s.name)}
        >
          <div class="card-header">
            <span class="source-icon">⬡</span>
            <span class="source-name">{s.name}</span>
          </div>
          <div class="card-meta">
            <span class="url" title={s.url}>{s.url}</span>
          </div>
          <div class="card-footer">
            <span class="commit">
              <span class="commit-dot"></span>
              {(s.commit || "").slice(0, 8)}
            </span>
            <div class="card-actions">
              <button class="btn-ghost" onclick={(e) => { e.stopPropagation(); updateSource(s.name); }} disabled={loading} title="Pull latest">↻</button>
              <button class="btn-ghost btn-danger" onclick={(e) => { e.stopPropagation(); removeSource(s.name); }} disabled={loading} title="Remove">✕</button>
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

  .header-marker {
    color: var(--amber);
  }

  .count {
    font-size: 10px;
    background: var(--bg-surface);
    padding: 1px 7px;
    border-radius: 8px;
    color: var(--amber);
    border: 1px solid var(--border);
  }

  .add-row {
    display: flex;
    gap: 8px;
    margin-bottom: 20px;
  }

  .input-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }

  .input-wrap:focus-within {
    border-color: var(--amber);
    box-shadow: 0 0 0 2px var(--amber-glow);
  }

  .input-prefix {
    font-family: "JetBrains Mono", monospace;
    color: var(--amber);
    padding: 0 0 0 12px;
    font-weight: 600;
    font-size: 13px;
  }

  .input-wrap input {
    border: none;
    background: transparent;
    flex: 1;
    padding: 8px 12px 8px 8px;
  }

  .input-wrap input:focus {
    box-shadow: none;
  }

  .btn-primary {
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.1em;
    background: var(--amber);
    color: var(--bg-deep);
    border: none;
    padding: 8px 20px;
  }

  .btn-primary:hover {
    background: #9a60f8;
    color: var(--bg-deep);
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    margin-bottom: 16px;
    border-radius: 3px;
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    animation: slideIn 0.2s ease-out;
  }

  .toast.error {
    background: rgba(224, 64, 64, 0.1);
    border: 1px solid rgba(224, 64, 64, 0.3);
    color: var(--red);
  }

  .toast.success {
    background: rgba(64, 192, 96, 0.1);
    border: 1px solid rgba(64, 192, 96, 0.3);
    color: var(--green);
  }

  .toast-icon {
    font-weight: 700;
  }

  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-8px); }
    to { opacity: 1; transform: translateY(0); }
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

  .empty-icon {
    font-size: 32px;
    color: var(--border-bright);
    margin-bottom: 8px;
  }

  .hint {
    font-size: 12px;
    opacity: 0.6;
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

  .source-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .source-card {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 14px 16px;
    cursor: pointer;
    transition: border-color 0.2s, box-shadow 0.2s;
    animation: fadeUp 0.3s ease-out both;
  }

  .source-card:hover {
    border-color: var(--border-bright);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.3);
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .source-icon {
    color: var(--amber);
    font-size: 12px;
  }

  .source-name {
    font-family: "JetBrains Mono", monospace;
    font-weight: 600;
    font-size: 13px;
    color: var(--text-bright);
  }

  .card-meta {
    margin-bottom: 10px;
  }

  .url {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .commit {
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .commit-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 4px var(--green-dim);
  }

  .card-actions {
    display: flex;
    gap: 4px;
  }

  .btn-ghost {
    background: transparent;
    border: 1px solid transparent;
    padding: 4px 8px;
    font-size: 13px;
    color: var(--text-dim);
    border-radius: 3px;
  }

  .btn-ghost:hover {
    background: var(--bg-surface);
    border-color: var(--border);
    color: var(--text);
  }

  .btn-danger:hover {
    color: var(--red);
    border-color: rgba(224, 64, 64, 0.3);
    background: rgba(224, 64, 64, 0.08);
  }
</style>
