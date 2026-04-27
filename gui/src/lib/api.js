import { invoke } from "@tauri-apps/api/core";

async function callApi(command, args = {}) {
  const result = await invoke("call_api", {
    request: { command, args },
  });
  if (!result.success) {
    throw new Error(result.error || "Unknown error");
  }
  return result.data;
}

export const api = {
  call: callApi,
  config: {
    export: () => callApi("config.export"),
    import: (config, force = false) => callApi("config.import", { config, force }),
  },
  app: {
    checkUpdate: (current) => callApi("app.check_update", { current }),
  },
  sources: {
    list: () => callApi("source.list"),
    add: (url, name) => callApi("source.add", { url, name }),
    remove: (name) => callApi("source.remove", { name }),
    update: (name) => callApi("source.update", name ? { name } : {}),
    checkUpdates: () => callApi("source.check_updates"),
  },
  plugins: {
    list: (opts = {}) => callApi("plugin.list", opts),
    detail: (name, source = null) => callApi("plugin.detail", source ? { name, source } : { name }),
    add: (name, opts = {}) => callApi("plugin.add", { name, ...opts }),
    delete: (name) => callApi("plugin.delete", { name }),
    update: (name) => callApi("plugin.update", name ? { name } : { all: true }),
    toggle: (name, { component, only_types, enable }) => callApi("plugin.toggle", { name, component, only_types, enable }),
  },
};
