// Update check state management.
// Checks on window focus, rate-limited to once per 10 minutes.

import { writable } from "svelte/store";
import { api } from "$lib/api.js";

export const appUpdate = writable(null); // { latest, has_update, download_url, release_url }
export const sourceUpdates = writable({}); // { [sourceName]: { has_update, latest } }

const COOLDOWN_MS = 10 * 60 * 1000; // 10 minutes
let lastCheck = 0;

export async function checkUpdates(currentVersion) {
  const now = Date.now();
  if (now - lastCheck < COOLDOWN_MS) return;
  lastCheck = now;

  // Run both checks in parallel in the background
  Promise.allSettled([
    api.call("app.check_update", { current: currentVersion }).then(data => {
      if (data?.has_update) {
        appUpdate.set(data);
      }
    }).catch(e => console.warn("app update check failed:", e)),

    api.call("source.check_updates", {}).then(data => {
      const map = {};
      for (const s of data || []) {
        if (s.has_update) {
          map[s.name] = { latest: s.latest, current: s.current };
        }
      }
      sourceUpdates.set(map);
    }).catch(e => console.warn("source update check failed:", e)),
  ]);
}
