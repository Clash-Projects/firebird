import type { PluginRecord, ScopedPluginStorage } from './types';

const REGISTRY_KEY = 'lw_plugins_registry_v1';

export function createPluginStorage(pluginId: string): ScopedPluginStorage {
  const prefix = `lw_plugin:${pluginId}:`;
  return Object.freeze({
    get<T = unknown>(key: string, fallback?: T): T | undefined {
      try {
        const raw = localStorage.getItem(prefix + key);
        if (raw === null) return fallback;
        return JSON.parse(raw) as T;
      } catch {
        return fallback;
      }
    },
    set(key: string, value: unknown) {
      try {
        localStorage.setItem(prefix + key, JSON.stringify(value));
      } catch {
        // Ignore storage quota errors in standalone mode.
      }
    },
    remove(key: string) {
      try {
        localStorage.removeItem(prefix + key);
      } catch {
        // Ignore storage errors.
      }
    },
  });
}

export function loadSavedPluginRecords(): PluginRecord[] {
  try {
    const raw = localStorage.getItem(REGISTRY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as PluginRecord[]) : [];
  } catch {
    return [];
  }
}

export function savePluginRecords(records: PluginRecord[]) {
  try {
    const serializable = records.map(({ manifest, enabled, builtin, sourceCode, cssCode }) => ({
      manifest,
      enabled,
      builtin,
      sourceCode,
      cssCode,
    }));
    localStorage.setItem(REGISTRY_KEY, JSON.stringify(serializable));
  } catch {
    // Ignore storage quota errors.
  }
}
