import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { featuresStore } from '$src/state/features.svelte';
import { installLastWaveApi } from './api';
import { BUILTIN_PLUGINS } from './builtins';
import { loadSavedPluginRecords, savePluginRecords } from './storage';
import {
  applyPaletteCss,
  applySnippetsCss,
  applyThemeCss,
  extractPaletteSwatch,
} from './themeLoader';
import type {
  MarketplacePluginEntry,
  PluginManifest,
  PluginRecord,
  PluginType,
} from './types';
import { uiSlots } from './uiSlots.svelte';

const VALID_TYPES = new Set<PluginType>(['palette', 'theme', 'snippet', 'widget']);
const SUPPORTED_API_VERSION = 1;
const REGISTRY_PRIMARY_URL =
  'https://raw.githubusercontent.com/Clash-Projects/lastwave-plugins/main/registry.json';
const REGISTRY_FALLBACK_URL =
  'https://cdn.jsdelivr.net/gh/Clash-Projects/lastwave-plugins@main/registry.json';
const RAW_REPO_BASE = 'https://raw.githubusercontent.com/Clash-Projects/lastwave-plugins/main';
const CDN_REPO_BASE = 'https://cdn.jsdelivr.net/gh/Clash-Projects/lastwave-plugins@main';

interface DiskPluginEntry {
  manifest: PluginManifest;
  enabled: boolean;
  content: string;
}

interface DiskPluginsState {
  safeMode: boolean;
  disabledAll: boolean;
  pluginsDir: string;
  plugins: DiskPluginEntry[];
}

function normalizeManifest(raw: Partial<PluginManifest> & { type?: string }): PluginManifest | null {
  if (!raw || typeof raw !== 'object') return null;
  if (!raw.id || !raw.name || !raw.type || !VALID_TYPES.has(raw.type as PluginType)) {
    return null;
  }
  const type = raw.type as PluginType;
  const defaultEntry =
    type === 'palette'
      ? 'colors.json'
      : type === 'theme'
        ? 'theme.css'
        : type === 'snippet'
          ? 'snippet.css'
          : 'widget.js';

  return {
    id: String(raw.id),
    name: String(raw.name),
    version: String(raw.version || '1.0.0'),
    apiVersion: typeof raw.apiVersion === 'number' ? raw.apiVersion : 1,
    minAppVersion: raw.minAppVersion ? String(raw.minAppVersion) : undefined,
    author: String(raw.author || 'Community'),
    description: String(raw.description || ''),
    type,
    entry: String(raw.entry || raw.css || defaultEntry),
    swatch: raw.swatch,
    sourceUrl: raw.sourceUrl,
  };
}

class PluginManager {
  plugins = $state<PluginRecord[]>([]);
  marketplace = $state<MarketplacePluginEntry[]>([]);
  marketplaceLoading = $state(false);
  marketplaceError = $state<string | null>(null);
  safeMode = $state(false);
  allDisabled = $state(false);
  pluginsDir = $state('');
  activePaletteId = $state<string | null>(null);
  activeThemeId = $state<string | null>(null);
  private initialized = false;

  activeWidgets = $derived(
    this.safeMode || this.allDisabled
      ? []
      : this.plugins.filter(
          (p) =>
            p.manifest.type === 'widget' &&
            p.enabled &&
            p.manifest.apiVersion === SUPPORTED_API_VERSION &&
            Boolean(p.sourceCode)
        )
  );

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    installLastWaveApi();

    const byId = new Map<string, PluginRecord>();
    for (const b of BUILTIN_PLUGINS) byId.set(b.manifest.id, { ...b });

    if (isTauriRuntime()) {
      const disk = await ipcInvoke<DiskPluginsState>('plugins_state');
      if (disk) {
        this.safeMode = Boolean(disk.safeMode);
        this.allDisabled = Boolean(disk.disabledAll);
        this.pluginsDir = disk.pluginsDir || '';
        for (const item of disk.plugins ?? []) {
          const manifest = normalizeManifest(item.manifest);
          if (!manifest) continue;
          const isCss = manifest.type !== 'widget';
          if (manifest.type === 'palette' && !manifest.swatch) {
            manifest.swatch = extractPaletteSwatch(item.content);
          }
          byId.set(manifest.id, {
            manifest,
            enabled: Boolean(item.enabled),
            builtin: false,
            cssCode: isCss ? item.content : undefined,
            sourceCode: isCss ? undefined : item.content,
            error:
              manifest.apiVersion !== SUPPORTED_API_VERSION
                ? `Incompatible apiVersion ${manifest.apiVersion} (requires ${SUPPORTED_API_VERSION})`
                : null,
          });
        }
      }
    }

    const saved = loadSavedPluginRecords().filter((r) => VALID_TYPES.has(r.manifest?.type));
    for (const s of saved) {
      const existing = byId.get(s.manifest.id);
      if (existing) {
        existing.enabled = s.enabled;
      } else if (!isTauriRuntime()) {
        const manifest = normalizeManifest(s.manifest);
        if (manifest) byId.set(manifest.id, { ...s, manifest });
      }
    }

    this.plugins = Array.from(byId.values());
    this.reapplyActiveMods();
  }

  private checkCompatibility(rec: PluginRecord): boolean {
    if (rec.manifest.apiVersion !== SUPPORTED_API_VERSION) {
      const msg = `${rec.manifest.name} requires apiVersion ${rec.manifest.apiVersion} (app supports ${SUPPORTED_API_VERSION})`;
      rec.error = msg;
      rec.enabled = false;
      featuresStore.showToast({
        title: 'Incompatible plugin',
        subtitle: msg,
        severity: 'warning',
      });
      return false;
    }
    rec.error = null;
    return true;
  }

  private reapplyActiveMods() {
    if (this.safeMode || this.allDisabled) {
      this.activePaletteId = null;
      this.activeThemeId = null;
      applyPaletteCss(null);
      applyThemeCss(null);
      applySnippetsCss([]);
      return;
    }

    const palette = this.plugins.find(
      (p) => p.manifest.type === 'palette' && p.enabled && this.checkCompatibility(p)
    );
    this.activePaletteId = palette?.manifest.id ?? null;
    applyPaletteCss(palette?.cssCode ?? null);

    const theme = this.plugins.find(
      (p) => p.manifest.type === 'theme' && p.enabled && this.checkCompatibility(p)
    );
    this.activeThemeId = theme?.manifest.id ?? null;
    applyThemeCss(theme?.cssCode ?? null);

    this.syncSnippets();
  }

  private syncSnippets() {
    if (this.safeMode || this.allDisabled) {
      applySnippetsCss([]);
      return;
    }
    const cssList = this.plugins
      .filter(
        (p) =>
          p.manifest.type === 'snippet' &&
          p.enabled &&
          p.cssCode &&
          p.manifest.apiVersion === SUPPORTED_API_VERSION
      )
      .map((p) => p.cssCode!);
    applySnippetsCss(cssList);
  }

  async setAllDisabled(disabled: boolean) {
    this.allDisabled = disabled;
    if (isTauriRuntime()) {
      await ipcInvoke('plugins_set_all_disabled', { disabled });
    }
    this.reapplyActiveMods();
    featuresStore.showToast({
      title: disabled ? 'All plugins disabled' : 'Plugins re-enabled',
      severity: disabled ? 'warning' : 'success',
    });
  }

  activatePalette(id: string | null) {
    if (id) {
      const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'palette');
      if (!target || !this.checkCompatibility(target)) return;
    }
    this.activePaletteId = id;
    for (const rec of this.plugins) {
      if (rec.manifest.type === 'palette') {
        const next = rec.manifest.id === id;
        if (rec.enabled !== next) {
          rec.enabled = next;
          if (!rec.builtin && isTauriRuntime()) {
            void ipcInvoke('plugins_set_enabled', { id: rec.manifest.id, enabled: next });
          }
        }
      }
    }
    const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'palette');
    applyPaletteCss(this.safeMode || this.allDisabled ? null : (target?.cssCode ?? null));
    savePluginRecords(this.plugins);
  }

  activateTheme(id: string | null) {
    if (id) {
      const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'theme');
      if (!target || !this.checkCompatibility(target)) return;
    }
    this.activeThemeId = id;
    for (const rec of this.plugins) {
      if (rec.manifest.type === 'theme') {
        const next = rec.manifest.id === id;
        if (rec.enabled !== next) {
          rec.enabled = next;
          if (!rec.builtin && isTauriRuntime()) {
            void ipcInvoke('plugins_set_enabled', { id: rec.manifest.id, enabled: next });
          }
        }
      }
    }
    const target = this.plugins.find((p) => p.manifest.id === id && p.manifest.type === 'theme');
    applyThemeCss(this.safeMode || this.allDisabled ? null : (target?.cssCode ?? null));
    savePluginRecords(this.plugins);
  }

  async togglePlugin(id: string) {
    const rec = this.plugins.find((p) => p.manifest.id === id);
    if (!rec) return;

    if (!rec.enabled && !this.checkCompatibility(rec)) {
      return;
    }

    if (rec.manifest.type === 'palette') {
      this.activatePalette(rec.enabled ? null : id);
      return;
    }
    if (rec.manifest.type === 'theme') {
      this.activateTheme(rec.enabled ? null : id);
      return;
    }

    rec.enabled = !rec.enabled;
    if (!rec.builtin && isTauriRuntime()) {
      await ipcInvoke('plugins_set_enabled', { id, enabled: rec.enabled });
    }
    if (rec.manifest.type === 'snippet') {
      this.syncSnippets();
    }
    savePluginRecords(this.plugins);
  }

  async installPlugin(manifestInput: PluginManifest, payload: { cssCode?: string; sourceCode?: string }) {
    const manifest = normalizeManifest(manifestInput);
    if (!manifest) {
      throw new Error('Invalid plugin manifest: only palette, theme, snippet, and widget are supported.');
    }
    if (manifest.apiVersion !== SUPPORTED_API_VERSION) {
      throw new Error(
        `Plugin "${manifest.name}" uses apiVersion ${manifest.apiVersion}, but LastWave supports apiVersion ${SUPPORTED_API_VERSION}.`
      );
    }

    const content = manifest.type === 'widget' ? payload.sourceCode ?? '' : payload.cssCode ?? '';
    if (manifest.type === 'palette' && !manifest.swatch) {
      manifest.swatch = extractPaletteSwatch(content);
    }

    if (isTauriRuntime()) {
      await ipcInvoke('plugins_install', { manifest, content });
    }

    const record: PluginRecord = {
      manifest,
      enabled: true,
      builtin: false,
      cssCode: manifest.type === 'widget' ? undefined : content,
      sourceCode: manifest.type === 'widget' ? content : undefined,
      error: null,
    };
    this.plugins = [record, ...this.plugins.filter((p) => p.manifest.id !== manifest.id)];
    if (manifest.type === 'palette') this.activatePalette(manifest.id);
    else if (manifest.type === 'theme') this.activateTheme(manifest.id);
    else if (manifest.type === 'snippet') this.syncSnippets();
    savePluginRecords(this.plugins);
  }

  async uninstallPlugin(id: string) {
    const rec = this.plugins.find((p) => p.manifest.id === id);
    if (!rec || rec.builtin) return;
    if (rec.manifest.type === 'palette' && this.activePaletteId === id) this.activatePalette(null);
    else if (rec.manifest.type === 'theme' && this.activeThemeId === id) this.activateTheme(null);
    if (isTauriRuntime()) {
      await ipcInvoke('plugins_uninstall', { id });
    }
    uiSlots.clearForPlugin(id);
    this.plugins = this.plugins.filter((p) => p.manifest.id !== id);
    if (rec.manifest.type === 'snippet') this.syncSnippets();
    savePluginRecords(this.plugins);
  }

  private async fetchRemoteText(url: string): Promise<string> {
    if (isTauriRuntime()) {
      const text = await ipcInvoke<string>('plugins_fetch_remote_text', { url });
      if (typeof text === 'string') return text;
    }
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${url}`);
    return await resp.text();
  }

  async fetchMarketplace() {
    this.marketplaceLoading = true;
    this.marketplaceError = null;
    try {
      let data: { plugins?: MarketplacePluginEntry[] } | null = null;
      if (isTauriRuntime()) {
        data = await ipcInvoke<{ plugins?: MarketplacePluginEntry[] }>('plugins_fetch_registry');
      }
      if (!data) {
        for (const url of [REGISTRY_PRIMARY_URL, REGISTRY_FALLBACK_URL]) {
          try {
            const resp = await fetch(url);
            if (resp.ok) {
              data = (await resp.json()) as { plugins?: MarketplacePluginEntry[] };
              break;
            }
          } catch {
            // Try fallback CDN.
          }
        }
      }
      if (!data || !Array.isArray(data.plugins)) {
        throw new Error('Could not load Clash-Projects/lastwave-plugins registry');
      }
      this.marketplace = data.plugins.filter((p) => VALID_TYPES.has(p.type));
    } catch (err) {
      this.marketplaceError = err instanceof Error ? err.message : 'Failed to load marketplace';
    } finally {
      this.marketplaceLoading = false;
    }
  }

  async installFromRegistry(entry: MarketplacePluginEntry) {
    if (entry.apiVersion !== SUPPORTED_API_VERSION) {
      featuresStore.showToast({
        title: 'Incompatible plugin',
        subtitle: `${entry.name} requires apiVersion ${entry.apiVersion}`,
        severity: 'warning',
      });
      return;
    }
    const basePrimary = `${RAW_REPO_BASE}/${entry.path}`;
    const baseFallback = `${CDN_REPO_BASE}/${entry.path}`;
    let manifestText = '';
    let entryText = '';
    let usedBase = basePrimary;

    try {
      manifestText = await this.fetchRemoteText(`${basePrimary}/manifest.json`);
    } catch {
      usedBase = baseFallback;
      manifestText = await this.fetchRemoteText(`${baseFallback}/manifest.json`);
    }

    const rawManifest = JSON.parse(manifestText) as Partial<PluginManifest>;
    const manifest = normalizeManifest({
      ...rawManifest,
      swatch: rawManifest.swatch ?? entry.swatch,
      sourceUrl: `${usedBase}/manifest.json`,
    });
    if (!manifest) throw new Error('Invalid plugin manifest in registry');

    entryText = await this.fetchRemoteText(`${usedBase}/${manifest.entry}`);
    await this.installPlugin(
      manifest,
      manifest.type === 'widget' ? { sourceCode: entryText } : { cssCode: entryText }
    );
    featuresStore.showToast({
      title: `Installed ${manifest.name}`,
      subtitle: `v${manifest.version} activated`,
      severity: 'success',
    });
  }

  async installFromUrl(input: string) {
    const clean = input.trim();
    if (!clean) throw new Error('Enter a GitHub owner/repo or manifest.json URL');

    let manifestUrl: string;
    if (clean.startsWith('https://')) {
      if (clean.endsWith('/manifest.json')) {
        manifestUrl = clean;
      } else if (clean.includes('github.com/')) {
        const parts = clean.replace(/^https:\/\/github\.com\//, '').replace(/\/+$/, '').split('/');
        if (parts.length < 2) throw new Error('Invalid GitHub repository URL');
        const [owner, repo, maybeTree, maybeBranch, ...rest] = parts;
        if (maybeTree === 'tree' && maybeBranch && rest.length > 0) {
          manifestUrl = `https://raw.githubusercontent.com/${owner}/${repo}/${maybeBranch}/${rest.join('/')}/manifest.json`;
        } else {
          manifestUrl = `https://raw.githubusercontent.com/${owner}/${repo}/main/manifest.json`;
        }
      } else {
        manifestUrl = `${clean.replace(/\/+$/, '')}/manifest.json`;
      }
    } else {
      const parts = clean.replace(/^\/+|\/+$/g, '').split('/');
      if (parts.length < 2) throw new Error('Use owner/repo or a full https:// manifest URL');
      const [owner, repo, ...sub] = parts;
      const suffix = sub.length > 0 ? `/${sub.join('/')}` : '';
      manifestUrl = `https://raw.githubusercontent.com/${owner}/${repo}/main${suffix}/manifest.json`;
    }

    const manifestText = await this.fetchRemoteText(manifestUrl);
    const rawManifest = JSON.parse(manifestText) as Partial<PluginManifest>;
    const manifest = normalizeManifest({ ...rawManifest, sourceUrl: manifestUrl });
    if (!manifest) throw new Error('Remote manifest.json is invalid or unsupported');

    const baseUrl = manifestUrl.slice(0, manifestUrl.lastIndexOf('/'));
    const entryText = await this.fetchRemoteText(`${baseUrl}/${manifest.entry}`);
    await this.installPlugin(
      manifest,
      manifest.type === 'widget' ? { sourceCode: entryText } : { cssCode: entryText }
    );
    featuresStore.showToast({
      title: `Installed ${manifest.name}`,
      subtitle: `Loaded from ${manifest.author}`,
      severity: 'success',
    });
  }
}

export const pluginManager = new PluginManager();
