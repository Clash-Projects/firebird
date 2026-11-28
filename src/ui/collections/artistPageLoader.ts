import { Channel } from '@tauri-apps/api/core';
import { ipcInvoke } from '$src/lib/ipc';
import type { mapYtArtist, mapYtTrack } from '$src/state/library.svelte';
import type { ArtistReleaseShelf } from './artistReleases';

type ArtistSong = Parameters<typeof mapYtTrack>[0];
type YtArtistResult = Parameters<typeof mapYtArtist>[0];

export interface ArtistPageResponse {
  name?: string;
  bannerUrl?: string;
  songs?: ArtistSong[];
  shelves?: ArtistReleaseShelf[];
}

type ArtistPageUpdate =
  | { kind: 'preview'; data: ArtistPageResponse }
  | { kind: 'songs'; data: ArtistSong[] }
  | { kind: 'shelf'; data: { index: number; shelf: ArtistReleaseShelf } };

type PageListener = (page: ArtistPageResponse | null, complete: boolean) => void;

interface ArtistRequest {
  page: ArtistPageResponse | null;
  listeners: Set<PageListener>;
}

const CACHE_TTL = 5 * 60 * 1000;
const CACHE_LIMIT = 24;
const cache = new Map<string, { page: ArtistPageResponse; expiresAt: number }>();
const pending = new Map<string, ArtistRequest>();
const nameCache = new Map<string, string>();
const motionCache = new Map<string, { result: Promise<string | null>; expiresAt: number }>();

export async function resolveArtistBrowseId(name: string): Promise<string | null> {
  const clean = name.trim();
  if (!clean) return null;
  const key = clean.toLowerCase();
  const cached = nameCache.get(key);
  if (cached) return cached;

  const results = await ipcInvoke<YtArtistResult[]>('yt_search_artists', { query: clean, limit: 5 });
  if (!Array.isArray(results)) return null;
  const withId = results.filter((entry) => Boolean(entry.browseId));
  const exact = withId.find((entry) => entry.name?.trim().toLowerCase() === key);
  const match = exact ?? withId[0];
  if (!match?.browseId) return null;
  nameCache.set(key, match.browseId);
  return match.browseId;
}

export function loadArtistBannerMotion(artistId: string, artist: string, trackTitles: string[]): Promise<string | null> {
  const key = JSON.stringify([artistId, artist, trackTitles]);
  const cached = motionCache.get(key);
  if (cached && cached.expiresAt > Date.now()) return cached.result;

  motionCache.delete(key);
  if (motionCache.size >= CACHE_LIMIT) {
    const oldest = motionCache.keys().next().value;
    if (oldest !== undefined) motionCache.delete(oldest);
  }
  const result = ipcInvoke<string>('artwork_lookup_artist_banner', { artist, trackTitles });
  motionCache.set(key, { result, expiresAt: Date.now() + CACHE_TTL });
  return result;
}

export function subscribeToArtistPage(browseId: string, accountScope: string, listener: PageListener): () => void {
  const key = JSON.stringify([accountScope, browseId]);
  const cached = cache.get(key);
  if (cached && cached.expiresAt > Date.now()) {
    listener(cached.page, true);
    return () => {};
  }
  cache.delete(key);

  const active = pending.get(key);
  if (active) {
    active.listeners.add(listener);
    if (active.page) listener(active.page, false);
    return () => active.listeners.delete(listener);
  }

  const request: ArtistRequest = { page: null, listeners: new Set([listener]) };
  pending.set(key, request);

  const updates = new Channel<ArtistPageUpdate>((update) => {
    if (update.kind === 'preview') {
      request.page = update.data;
    } else if (request.page && update.kind === 'songs') {
      request.page = { ...request.page, songs: update.data };
    } else if (request.page && update.kind === 'shelf') {
      const { index, shelf } = update.data;
      const shelves = request.page.shelves?.map((entry, i) => i === index ? shelf : entry);
      request.page = { ...request.page, shelves };
    }
    if (request.page) {
      for (const notify of request.listeners) notify(request.page, false);
    }
  });

  void ipcInvoke<ArtistPageResponse>('yt_fetch_artist_page', { browseId, updates }).then((result) => {
    pending.delete(key);
    const page = result ?? request.page;
    if (result) {
      if (cache.size >= CACHE_LIMIT) {
        const oldest = cache.keys().next().value;
        if (oldest !== undefined) cache.delete(oldest);
      }
      cache.set(key, { page: result, expiresAt: Date.now() + CACHE_TTL });
    }
    for (const notify of request.listeners) notify(page, true);
    request.listeners.clear();
  });

  return () => request.listeners.delete(listener);
}
