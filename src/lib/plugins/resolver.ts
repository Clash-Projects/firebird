import type { PluginAuthor, PluginSource } from './types';

const OFFICIAL_RAW_BASE = 'https://raw.githubusercontent.com/Clash-Projects/lastwave-plugins/main';
const OFFICIAL_CDN_BASE = 'https://cdn.jsdelivr.net/gh/Clash-Projects/lastwave-plugins@main';

export interface AssetSourceRef {
  source?: PluginSource;
  path?: string;
  repo?: string;
  commit?: string;
  subpath?: string;
}

export function normalizeAuthorObject(author: PluginAuthor | undefined): { name: string; github: string } {
  if (author && typeof author === 'object') {
    const name = String(author.name || author.github || 'Community').trim();
    const github = String(author.github || author.name || 'community').replace(/^@/, '').trim();
    return { name, github };
  }
  const text = String(author || 'Community').trim();
  const handle = text.replace(/^@/, '').replace(/\s+/g, '');
  return { name: text || 'Community', github: handle || 'community' };
}

export function formatAuthorName(author: PluginAuthor | undefined): string {
  return normalizeAuthorObject(author).name;
}

export function extractAuthorGithub(author: PluginAuthor | undefined): string {
  return normalizeAuthorObject(author).github;
}

export function isOfficialPlugin(item: { source?: PluginSource; author?: PluginAuthor }): boolean {
  return item.source === 'official' || extractAuthorGithub(item.author) === 'Clash-Projects';
}

function cleanRelativePath(rel: string): string | null {
  const clean = rel.trim().replace(/^(\.\/|\/)+/, '').replace(/\/+$/, '');
  if (!clean || clean.includes('..') || clean.includes('\\')) return null;
  return clean;
}

export function normalizePreviews(raw: { previews?: unknown; preview?: unknown }): string[] {
  const list: string[] = [];
  if (Array.isArray(raw.previews)) {
    for (const item of raw.previews) {
      if (typeof item === 'string') {
        const cleaned = cleanRelativePath(item);
        if (cleaned) list.push(cleaned);
      }
    }
  }
  if (list.length === 0 && typeof raw.preview === 'string') {
    const cleaned = cleanRelativePath(raw.preview);
    if (cleaned) list.push(cleaned);
  }
  return list.slice(0, 10);
}

export function resolvePluginAssetUrl(
  entry: AssetSourceRef,
  relativePath: string,
  useFallback = false
): string | null {
  const cleanRel = cleanRelativePath(relativePath);
  if (!cleanRel) return null;

  if (entry.source === 'external' || (entry.repo && entry.commit)) {
    const repo = String(entry.repo || '').replace(/^\/+|\/+$/g, '');
    const commit = String(entry.commit || '').trim();
    if (!repo || !commit) return null;
    const sub = entry.subpath ? `/${String(entry.subpath).replace(/^\/+|\/+$/g, '')}` : '';
    if (useFallback) {
      return `https://cdn.jsdelivr.net/gh/${repo}@${commit}${sub}/${cleanRel}`;
    }
    return `https://raw.githubusercontent.com/${repo}/${commit}${sub}/${cleanRel}`;
  }

  const relFolder = String(entry.path || '').replace(/^\/+|\/+$/g, '');
  if (!relFolder) return null;
  const base = useFallback ? OFFICIAL_CDN_BASE : OFFICIAL_RAW_BASE;
  return `${base}/${relFolder}/${cleanRel}`;
}

export function resolveGithubBrowseUrl(entry: AssetSourceRef & { sourceUrl?: string }): string {
  if (entry.source === 'external' || (entry.repo && entry.commit)) {
    const repo = String(entry.repo || '').replace(/^\/+|\/+$/g, '');
    const commit = String(entry.commit || 'main').trim();
    const sub = entry.subpath ? `/${String(entry.subpath).replace(/^\/+|\/+$/g, '')}` : '';
    if (repo) {
      return `https://github.com/${repo}/tree/${commit}${sub}`;
    }
  }
  if (entry.path) {
    return `https://github.com/Clash-Projects/lastwave-plugins/tree/main/${entry.path.replace(/^\/+|\/+$/g, '')}`;
  }
  if (entry.sourceUrl?.includes('raw.githubusercontent.com/')) {
    const parts = entry.sourceUrl.replace('https://raw.githubusercontent.com/', '').split('/');
    if (parts.length >= 3) {
      const [owner, repo, ref, ...rest] = parts;
      const folder = rest.slice(0, -1).join('/');
      return `https://github.com/${owner}/${repo}/tree/${ref}${folder ? `/${folder}` : ''}`;
    }
  }
  return 'https://github.com/Clash-Projects/lastwave-plugins';
}
