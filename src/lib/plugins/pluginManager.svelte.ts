import { installLastWaveApi } from './api';
import { BUILTIN_PLUGINS } from './builtins';
import { loadSavedPluginRecords, savePluginRecords } from './storage';
import { applyPaletteCss, applySnippetsCss, applyThemeCss } from './themeLoader';
import type { LastWaveAPI, PluginManifest, PluginRecord } from './types';
import { uiSlots } from './uiSlots.svelte';

interface LoadedModule {
  init?: (api: LastWaveAPI) => void | (() => void) | Promise<void | (() => void)>;
  destroy?: () => void;
}

const VALID_TYPES = new Set(['palette', 'theme', 'snippet', 'widget']);

class PluginManager {
  plugins = $state<PluginRecord[]>([]);
  activePaletteId = $state<string | null>(null);
  activeThemeId = $state<string | null>(null);
  private cleanups = new Map<string, () => void>();
  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    installLastWaveApi();

    const saved = loadSavedPluginRecords().filter((r) => VALID_TYPES.has(r.manifest?.type));
    const byId = new Map<string, PluginRecord>();
    for (const b of BUILTIN_PLUGINS) byId.set(b.manifest.id, { ...b });
    for (const s of saved) {
      const existing = byId.get(s.manifest.id);
      byId.set(s.manifest.id, existing ? { ...existing, enabled: s.enabled } : s);
    }
    this.plugins = Array.from(byId.values());

    for (const rec of this.plugins) {
      if (!rec.enabled) continue;
      if (rec.manifest.type === 'palette') this.activatePalette(rec.manifest.id);
      else if (rec.manifest.type === 'theme') this.activateTheme(rec.manifest.id);
      else if (rec.manifest.type === 'widget') await this.loadWidgetPlugin(rec);
    }
    this.syncSnippets();
  }

  private syncSnippets() {
    const cssList = this.plugins
      .filter((p) => p.manifest.type === 'snippet' && p.enabled && p.cssCode)
      .map((p) => p.cssCode!);
    applySnippetsCss(cssList);
  }

  private async loadWidgetPlugin(rec: PluginRecord) {
    this.unloadWidgetPlugin(rec.manifest.id);
    if (!rec.sourceCode) return;
    const api = installLastWaveApi();
    const blob = new Blob([rec.sourceCode], { type: 'text/javascript' });
    const url = URL.createObjectURL(blob);
    try {
      const mod = (await import(/* @vite-ignore */ url)) as LoadedModule;
      const cleanup = await mod.init?.(api);
      this.cleanups.set(rec.manifest.id, () => {
        try {
          if (typeof cleanup === 'function') cleanup();
          mod.destroy?.();
        } finally {
          uiSlots.clearForPlugin(rec.manifest.id);
        }
      });
      rec.error = null;
    } catch (err) {
      rec.error = err instanceof Error ? err.message : String(err);
      uiSlots.clearForPlugin(rec.manifest.id);
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  private unloadWidgetPlugin(id: string) {
    const fn = this.cleanups.get(id);
    if (fn) {
      try {
        fn();
      } catch {
        // Isolate teardown errors.
      }
      this.cleanups.delete(id);
    }
    uiSlots.clearForPlugin(id);
  }

  activatePalette(id: string | null) {
    this.activePaletteId = id;
    for (const rec of this.plugins) {
      if (rec.manifest.type === 'palette') rec.enabled = rec.manifest.id === id;
    }
    const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'palette');
    applyPaletteCss(target?.cssCode ?? null);
    savePluginRecords(this.plugins);
  }

  activateTheme(id: string | null) {
    this.activeThemeId = id;
    for (const rec of this.plugins) {
      if (rec.manifest.type === 'theme') rec.enabled = rec.manifest.id === id;
    }
    const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'theme');
    applyThemeCss(target?.cssCode ?? null);
    savePluginRecords(this.plugins);
  }

  async togglePlugin(id: string) {
    const rec = this.plugins.find((p) => p.manifest.id === id);
    if (!rec) return;

    if (rec.manifest.type === 'palette') {
      this.activatePalette(rec.enabled ? null : id);
      return;
    }
    if (rec.manifest.type === 'theme') {
      this.activateTheme(rec.enabled ? null : id);
      return;
    }

    rec.enabled = !rec.enabled;
    if (rec.manifest.type === 'snippet') {
      this.syncSnippets();
    } else if (rec.manifest.type === 'widget') {
      if (rec.enabled) await this.loadWidgetPlugin(rec);
      else this.unloadWidgetPlugin(id);
    }
    savePluginRecords(this.plugins);
  }

  async reloadPlugin(id: string) {
    const rec = this.plugins.find((p) => p.manifest.id === id);
    if (!rec || !rec.enabled) return;
    if (rec.manifest.type === 'palette') applyPaletteCss(rec.cssCode ?? null);
    else if (rec.manifest.type === 'theme') applyThemeCss(rec.cssCode ?? null);
    else if (rec.manifest.type === 'snippet') this.syncSnippets();
    else await this.loadWidgetPlugin(rec);
  }

  async installPlugin(manifest: PluginManifest, payload: { cssCode?: string; sourceCode?: string }) {
    const record: PluginRecord = {
      manifest,
      enabled: true,
      builtin: false,
      cssCode: payload.cssCode,
      sourceCode: payload.sourceCode,
      error: null,
    };
    this.plugins = [record, ...this.plugins.filter((p) => p.manifest.id !== manifest.id)];
    if (manifest.type === 'palette') this.activatePalette(manifest.id);
    else if (manifest.type === 'theme') this.activateTheme(manifest.id);
    else if (manifest.type === 'snippet') this.syncSnippets();
    else await this.loadWidgetPlugin(record);
    savePluginRecords(this.plugins);
  }

  uninstallPlugin(id: string) {
    const rec = this.plugins.find((p) => p.manifest.id === id);
    if (!rec || rec.builtin) return;
    if (rec.manifest.type === 'palette' && this.activePaletteId === id) this.activatePalette(null);
    else if (rec.manifest.type === 'theme' && this.activeThemeId === id) this.activateTheme(null);
    else if (rec.manifest.type === 'widget') this.unloadWidgetPlugin(id);
    this.plugins = this.plugins.filter((p) => p.manifest.id !== id);
    if (rec.manifest.type === 'snippet') this.syncSnippets();
    savePluginRecords(this.plugins);
  }
}

export const pluginManager = new PluginManager();
