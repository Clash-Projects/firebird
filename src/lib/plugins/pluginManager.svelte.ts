import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { featuresStore } from '$src/state/features.svelte';
import { installLastWaveApi } from './api';
import { BUILTIN_PLUGINS } from './builtins';
import {
  normalizeAuthorObject,
  normalizePreviews,
  resolvePluginAssetUrl,
  type AssetSourceRef,
} from './resolver';
import { loadSavedPluginRecords, savePluginRecords } from './storage';
import {
  applyPaletteCss,
  applySnippetsCss,
  applyThemeCss,
  extractPaletteSwatch,
  parsePaletteTokens,
} from './themeLoader';
import type {
  HydratedPluginDetails,
  MarketplacePluginEntry,
  PluginManifest,
  PluginRecord,
  PluginSource,
  PluginType,
} from './types';
import { uiSlots } from './uiSlots.svelte';

const VALID_TYPES = new Set<PluginType>(['palette', 'theme', 'snippet', 'widget']);
const SUPPORTED_API_VERSION = 1;
const REGISTRY_PRIMARY_URL =
  'https://raw.githubusercontent.com/Clash-Projects/lastwave-plugins/main/registry.json';
const REGISTRY_FALLBACK_URL =
  'https://cdn.jsdelivr.net/gh/Clash-Projects/lastwave-plugins@main/registry.json';

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

function defaultEntryFor(type: PluginType): string {
  if (type === 'palette') return 'colors.json';
  if (type === 'theme') return 'theme.css';
  if (type === 'snippet') return 'snippet.css';
  return 'widget.js';
}

function normalizeManifest(raw: Partial<PluginManifest> & { type?: string }): PluginManifest | null {
  if (!raw || typeof raw !== 'object') return null;
  if (!raw.id || !raw.name || !raw.type || !VALID_TYPES.has(raw.type as PluginType)) {
    return null;
  }
  const type = raw.type as PluginType;
  const previews = normalizePreviews(raw);

  return {
    id: String(raw.id),
    name: String(raw.name),
    version: String(raw.version || '1.0.0'),
    apiVersion: typeof raw.apiVersion === 'number' ? raw.apiVersion : 1,
    minAppVersion: raw.minAppVersion ? String(raw.minAppVersion) : undefined,
    author: normalizeAuthorObject(raw.author),
    description: String(raw.description || ''),
    type,
    entry: String(raw.entry || raw.css || defaultEntryFor(type)),
    tags: Array.isArray(raw.tags) ? raw.tags.map((t) => String(t).trim()).filter(Boolean) : [],
    previews,
    updatedAt: raw.updatedAt ? String(raw.updatedAt) : undefined,
    source: raw.source === 'external' ? 'external' : raw.source === 'official' ? 'official' : undefined,
    path: raw.path ? String(raw.path) : undefined,
    repo: raw.repo ? String(raw.repo) : undefined,
    commit: raw.commit ? String(raw.commit) : undefined,
    subpath: raw.subpath ? String(raw.subpath) : undefined,
    swatch: raw.swatch,
    sourceUrl: raw.sourceUrl,
  };
}

function normalizeMarketplaceEntry(raw: Partial<MarketplacePluginEntry> & Record<string, unknown>): MarketplacePluginEntry | null {
  if (!raw || typeof raw !== 'object') return null;
  if (!raw.id || !raw.name || !raw.type || !VALID_TYPES.has(raw.type as PluginType)) {
    return null;
  }
  const type = raw.type as PluginType;
  const source: PluginSource = raw.source === 'external' || (raw.repo && raw.commit) ? 'external' : 'official';
  return {
    id: String(raw.id),
    name: String(raw.name),
    type,
    source,
    version: String(raw.version || '1.0.0'),
    apiVersion: typeof raw.apiVersion === 'number' ? raw.apiVersion : 1,
    minAppVersion: raw.minAppVersion ? String(raw.minAppVersion) : undefined,
    author: normalizeAuthorObject(raw.author as PluginManifest['author']),
    description: String(raw.description || ''),
    tags: Array.isArray(raw.tags) ? raw.tags.map((t) => String(t).trim()).filter(Boolean) : [],
    previews: normalizePreviews(raw),
    updatedAt: typeof raw.updatedAt === 'string' && raw.updatedAt ? raw.updatedAt : '2026-11-27',
    path: typeof raw.path === 'string' ? raw.path : undefined,
    entry: typeof raw.entry === 'string' ? raw.entry : undefined,
    repo: typeof raw.repo === 'string' ? raw.repo : undefined,
    commit: typeof raw.commit === 'string' ? raw.commit : undefined,
    subpath: typeof raw.subpath === 'string' ? raw.subpath : '',
    swatch: Array.isArray(raw.swatch) && raw.swatch.length >= 3 ? (raw.swatch.slice(0, 3) as [string, string, string]) : undefined,
  };
}

class PluginManager {
  plugins = $state<PluginRecord[]>([]);
  marketplace = $state<MarketplacePluginEntry[]>([]);
  marketplaceLoading = $state(false);
  marketplaceError = $state<string | null>(null);
  detailsCache = $state<Record<string, HydratedPluginDetails>>({});
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

    const saved = loadSavedPluginRecords().filter(
      (r) => !r.builtin && VALID_TYPES.has(r.manifest?.type)
    );
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
    savePluginRecords(this.plugins);
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

  private async fetchAssetText(ref: AssetSourceRef, relativePath: string): Promise<string> {
    const primary = resolvePluginAssetUrl(ref, relativePath, false);
    if (!primary) throw new Error(`Invalid asset path: ${relativePath}`);
    try {
      return await this.fetchRemoteText(primary);
    } catch {
      const fallback = resolvePluginAssetUrl(ref, relativePath, true);
      if (!fallback || fallback === primary) throw new Error(`Failed to fetch ${primary}`);
      return await this.fetchRemoteText(fallback);
    }
  }

  async fetchMarketplace() {
    this.marketplaceLoading = true;
    this.marketplaceError = null;
    try {
      let data: { plugins?: Array<Partial<MarketplacePluginEntry> & Record<string, unknown>> } | null = null;
      if (isTauriRuntime()) {
        data = await ipcInvoke<{ plugins?: Array<Partial<MarketplacePluginEntry> & Record<string, unknown>> }>(
          'plugins_fetch_registry'
        );
      }
      if (!data) {
        for (const url of [REGISTRY_PRIMARY_URL, REGISTRY_FALLBACK_URL]) {
          try {
            const resp = await fetch(url);
            if (resp.ok) {
              data = (await resp.json()) as {
                plugins?: Array<Partial<MarketplacePluginEntry> & Record<string, unknown>>;
              };
              break;
            }
          } catch {
            // Try CDN fallback.
          }
        }
      }
      if (!data || !Array.isArray(data.plugins)) {
        throw new Error('Could not load Clash-Projects/lastwave-plugins registry');
      }
      this.marketplace = data.plugins
        .map((p) => normalizeMarketplaceEntry(p))
        .filter((p): p is MarketplacePluginEntry => p !== null);
    } catch (err) {
      this.marketplaceError = err instanceof Error ? err.message : 'Failed to load marketplace';
    } finally {
      this.marketplaceLoading = false;
    }
  }

  async hydratePluginDetails(item: MarketplacePluginEntry | PluginRecord): Promise<HydratedPluginDetails> {
    const isMarket = !('manifest' in item);
    const id = isMarket ? item.id : item.manifest.id;
    const key = isMarket ? `${id}@${item.commit || item.version}` : `${id}@${item.manifest.commit || item.manifest.version}`;
    const existing = this.detailsCache[key];
    if (existing && !existing.loading && (existing.manifest || existing.readme || existing.paletteTokens)) {
      return existing;
    }

    const initialTokens = !isMarket && item.cssCode ? parsePaletteTokens(item.cssCode) : undefined;
    this.detailsCache = {
      ...this.detailsCache,
      [key]: { loading: true, paletteTokens: initialTokens },
    };

    const ref: AssetSourceRef = isMarket
      ? {
          source: item.source,
          path: item.path,
          repo: item.repo,
          commit: item.commit,
          subpath: item.subpath,
        }
      : {
          source: item.manifest.source,
          path: item.manifest.path,
          repo: item.manifest.repo,
          commit: item.manifest.commit,
          subpath: item.manifest.subpath,
        };

    const canFetchRemote = Boolean(
      (ref.source === 'official' && ref.path) || (ref.source === 'external' && ref.repo && ref.commit)
    );

    if (!canFetchRemote) {
      const localResult: HydratedPluginDetails = {
        manifest: isMarket ? undefined : item.manifest,
        paletteTokens: initialTokens,
        loading: false,
      };
      this.detailsCache = { ...this.detailsCache, [key]: localResult };
      return localResult;
    }

    try {
      const manifestPromise = this.fetchAssetText(ref, 'manifest.json')
        .then((txt) => normalizeManifest(JSON.parse(txt) as Partial<PluginManifest>))
        .catch(() => null);
      const readmePromise = this.fetchAssetText(ref, 'README.md').catch(() => undefined);

      const [hydratedManifest, readme] = await Promise.all([manifestPromise, readmePromise]);
      let paletteTokens = initialTokens;
      const pluginType = isMarket ? item.type : item.manifest.type;
      const entryFile = hydratedManifest?.entry || (isMarket ? item.entry : item.manifest.entry) || defaultEntryFor(pluginType);

      if (!paletteTokens && pluginType === 'palette') {
        try {
          const entryRaw = await this.fetchAssetText(ref, entryFile);
          paletteTokens = parsePaletteTokens(entryRaw);
        } catch {
          // Ignore optional palette preview token fetch failures.
        }
      }

      const details: HydratedPluginDetails = {
        manifest: hydratedManifest ?? (isMarket ? undefined : item.manifest),
        readme,
        paletteTokens,
        loading: false,
      };
      this.detailsCache = { ...this.detailsCache, [key]: details };
      return details;
    } catch (err) {
      const failed: HydratedPluginDetails = {
        paletteTokens: initialTokens,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to load plugin metadata',
      };
      this.detailsCache = { ...this.detailsCache, [key]: failed };
      return failed;
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

    const ref: AssetSourceRef = {
      source: entry.source,
      path: entry.path,
      repo: entry.repo,
      commit: entry.commit,
      subpath: entry.subpath,
    };

    const manifestText = await this.fetchAssetText(ref, 'manifest.json');
    const rawManifest = JSON.parse(manifestText) as Partial<PluginManifest>;
    const manifestUrl = resolvePluginAssetUrl(ref, 'manifest.json') ?? undefined;
    const normalizedPreviews = normalizePreviews({
      previews: rawManifest.previews ?? entry.previews,
      preview: rawManifest.preview,
    });

    const manifest = normalizeManifest({
      ...rawManifest,
      author: rawManifest.author ?? entry.author,
      tags: rawManifest.tags?.length ? rawManifest.tags : entry.tags,
      previews: normalizedPreviews,
      updatedAt: rawManifest.updatedAt ?? entry.updatedAt,
      source: entry.source,
      path: entry.path,
      repo: entry.repo,
      commit: entry.commit,
      subpath: entry.subpath,
      swatch: rawManifest.swatch ?? entry.swatch,
      sourceUrl: manifestUrl,
    });
    if (!manifest) throw new Error('Invalid plugin manifest in registry');

    const entryText = await this.fetchAssetText(ref, manifest.entry || entry.entry || defaultEntryFor(manifest.type));
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

  private async resolveLatestCommitSha(owner: string, repo: string): Promise<string> {
    try {
      const apiRaw = await this.fetchRemoteText(
        `https://api.github.com/repos/${owner}/${repo}/commits?per_page=1`
      );
      const commits = JSON.parse(apiRaw) as Array<{ sha?: string }>;
      const sha = commits?.[0]?.sha;
      if (sha && /^[0-9a-f]{40}$/i.test(sha)) return sha.toLowerCase();
    } catch {
      // Fallback to default branch resolution via GitHub API repo metadata or main.
    }
    try {
      const repoRaw = await this.fetchRemoteText(`https://api.github.com/repos/${owner}/${repo}`);
      const info = JSON.parse(repoRaw) as { default_branch?: string };
      if (info.default_branch) return info.default_branch;
    } catch {
      // Fallback to main.
    }
    return 'main';
  }

  async installFromUrl(input: string) {
    const clean = input.trim();
    if (!clean) throw new Error('Enter owner/repo, owner/repo@commit, or a manifest.json URL');

    let manifestUrl: string;
    let sourceRef: AssetSourceRef | undefined;

    if (clean.startsWith('https://')) {
      if (clean.endsWith('/manifest.json')) {
        manifestUrl = clean;
      } else if (clean.includes('github.com/')) {
        const parts = clean.replace(/^https:\/\/github\.com\//, '').replace(/\/+$/, '').split('/');
        if (parts.length < 2) throw new Error('Invalid GitHub repository URL');
        const [owner, repo, maybeTree, maybeRef, ...rest] = parts;
        const commit =
          maybeTree === 'tree' && maybeRef ? maybeRef : await this.resolveLatestCommitSha(owner, repo);
        const subpath = maybeTree === 'tree' && rest.length > 0 ? rest.join('/') : '';
        sourceRef = { source: 'external', repo: `${owner}/${repo}`, commit, subpath };
        manifestUrl = resolvePluginAssetUrl(sourceRef, 'manifest.json')!;
      } else {
        manifestUrl = `${clean.replace(/\/+$/, '')}/manifest.json`;
      }
    } else {
      const [repoAndSub, explicitRef] = clean.replace(/^\/+|\/+$/g, '').split('@');
      const parts = repoAndSub.split('/').filter(Boolean);
      if (parts.length < 2) throw new Error('Use owner/repo, owner/repo@commit, or an https:// URL');
      const [owner, repo, ...sub] = parts;
      const subpath = sub.join('/');
      const commit = explicitRef?.trim() || (await this.resolveLatestCommitSha(owner, repo));
      sourceRef = { source: 'external', repo: `${owner}/${repo}`, commit, subpath };
      manifestUrl = resolvePluginAssetUrl(sourceRef, 'manifest.json')!;
    }

    const manifestText = await this.fetchRemoteText(manifestUrl);
    const rawManifest = JSON.parse(manifestText) as Partial<PluginManifest>;
    const manifest = normalizeManifest({
      ...rawManifest,
      source: sourceRef ? 'external' : rawManifest.source,
      repo: sourceRef?.repo ?? rawManifest.repo,
      commit: sourceRef?.commit ?? rawManifest.commit,
      subpath: sourceRef?.subpath ?? rawManifest.subpath,
      sourceUrl: manifestUrl,
    });
    if (!manifest) throw new Error('Remote manifest.json is invalid or unsupported');

    const baseUrl = manifestUrl.slice(0, manifestUrl.lastIndexOf('/'));
    const entryText = await this.fetchRemoteText(`${baseUrl}/${manifest.entry}`);
    await this.installPlugin(
      manifest,
      manifest.type === 'widget' ? { sourceCode: entryText } : { cssCode: entryText }
    );
    const authorLabel = normalizeAuthorObject(manifest.author).name;
    featuresStore.showToast({
      title: `Installed ${manifest.name}`,
      subtitle: `Loaded from ${authorLabel}`,
      severity: 'success',
    });
  }
}

export const pluginManager = new PluginManager();
