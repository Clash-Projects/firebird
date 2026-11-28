import { SvelteMap } from 'svelte/reactivity';
import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import type { Track } from './types';

type TrackMetadata = Pick<Track, 'album' | 'albumId' | 'durationSec'>;
interface SongSearchResult {
  videoId: string;
  title: string;
  artist: string;
  artworkUrl?: string;
  album?: string;
  albumBrowseId?: string;
  durationSeconds?: number;
}

interface AlbumRequest {
  track: Track;
  consumers: number;
  prefetched: boolean;
}

const cacheKey = 'lw_track_album_metadata';
const cacheLimit = 500;
const retryAfterMs = 5 * 60 * 1000;
const maxConcurrentLookups = 6;
const maxBackgroundLookups = 2;

function readCache(): [string, TrackMetadata][] {
  try {
    const raw = localStorage.getItem(cacheKey);
    if (raw) {
      const entries: [string, TrackMetadata][] = JSON.parse(raw);
      if (Array.isArray(entries)) return entries.slice(-cacheLimit).filter(([id, metadata]) =>
        typeof id === 'string' && typeof metadata?.album === 'string' && typeof metadata.durationSec === 'number',
      );
    }
  } catch {
    // Album metadata can be recovered again when the track is visible.
  }
  return [];
}

export const trackMetadataCache = new SvelteMap(readCache());
const pending = new Map<string, AlbumRequest>();
const attempted = new Map<string, number>();
let active = 0;
let backgroundActive = 0;

export function mergeTrackMetadata(track: Track, known?: TrackMetadata): Track {
  if (!known) return track;
  const compatible = !track.albumId || track.albumId === known.albumId;
  const album = track.album || (compatible ? known.album : '');
  const sameAlbum = !album || album.trim().toLowerCase() === known.album.trim().toLowerCase();
  const albumId = track.albumId || (sameAlbum ? known.albumId : undefined);
  const durationSec = track.durationSec || known.durationSec;
  if (album === track.album && albumId === track.albumId && durationSec === track.durationSec) return track;
  return { ...track, album, albumId, durationSec };
}

function remember(videoId: string, metadata: TrackMetadata) {
  if (trackMetadataCache.size >= cacheLimit) {
    const oldest = trackMetadataCache.keys().next().value;
    if (oldest !== undefined) trackMetadataCache.delete(oldest);
  }
  trackMetadataCache.set(videoId, metadata);
  try {
    localStorage.setItem(cacheKey, JSON.stringify([...trackMetadataCache]));
  } catch {
    // Keep the metadata available in this session.
  }
}

function normalize(value: string) {
  return value.normalize('NFKC').toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim();
}

function titleWithoutCredits(title: string) {
  return normalize(title.replace(/\s*\((?:feat(?:uring)?|ft)\.?\s[^)]*\)|\s*\[(?:feat(?:uring)?|ft)\.?\s[^\]]*\]/gi, ''));
}

function artistCredits(artist: string) {
  return artist.split(/[,;]|\s+&\s+/).map(normalize).filter(Boolean);
}

function artworkKey(source = '') {
  try {
    const url = new URL(source);
    if (/(^|\.)(googleusercontent|ggpht)\.com$/i.test(url.hostname)) {
      return url.pathname.replace(/=(?:w\d+|h\d+|s\d+).*$/i, '');
    }
  } catch {
    return '';
  }
  return '';
}

function identifyRelease(track: Track, results: SongSearchResult[]): TrackMetadata | undefined {
  const exact = results.find((song) => song.videoId === track.videoId && song.album?.trim());
  if (exact?.album) {
    return { album: exact.album.trim(), albumId: exact.albumBrowseId || undefined, durationSec: Math.max(0, exact.durationSeconds ?? 0) };
  }

  const title = titleWithoutCredits(track.title);
  const artist = artistCredits(track.artist)[0];
  if (!title || !artist || artist === 'unknown artist') return;
  const candidates = results.filter((song) => song.album?.trim() &&
    titleWithoutCredits(song.title) === title && artistCredits(song.artist).includes(artist) &&
    (!track.albumId || track.albumId === song.albumBrowseId) &&
    (!track.durationSec || !song.durationSeconds || Math.abs(track.durationSec - song.durationSeconds) <= 8));
  const artwork = artworkKey(track.artworkUrl);
  const sameCover = artwork ? candidates.filter((song) => artworkKey(song.artworkUrl) === artwork) : [];
  const releases = sameCover.length ? sameCover : candidates;
  const album = releases[0]?.album?.trim();
  if (!album || !releases.every((song) => normalize(song.album ?? '') === normalize(album))) return;
  const releaseIds = new Set(releases.map((song) => song.albumBrowseId).filter(Boolean));
  // An alternate upload can identify a release without proving its own runtime.
  return { album, albumId: releaseIds.size === 1 ? releaseIds.values().next().value : undefined, durationSec: 0 };
}

async function resolveAlbum(track: Track) {
  const results = await ipcInvoke<SongSearchResult[]>('yt_search_songs', { query: `${track.title} ${track.artist}`.trim(), limit: 12 });
  const metadata = Array.isArray(results) ? identifyRelease(track, results) : undefined;
  if (metadata) remember(track.videoId, metadata);
}

function startPendingLookups() {
  while (active < maxConcurrentLookups && pending.size) {
    const next = [...pending].find(([, request]) => request.consumers > 0)
      ?? (backgroundActive < maxBackgroundLookups ? pending.entries().next().value : undefined);
    if (!next) break;
    const [videoId, { track, consumers }] = next;
    const background = consumers === 0;
    pending.delete(videoId);
    attempted.set(videoId, Date.now());
    active += 1;
    if (background) backgroundActive += 1;
    void resolveAlbum(track).finally(() => {
      active -= 1;
      if (background) backgroundActive -= 1;
      startPendingLookups();
    });
  }
}

function canLookup(track: Track) {
  const { videoId } = track;
  if (!videoId || track.album || track.isLocal || trackMetadataCache.has(videoId) || !isTauriRuntime()) return false;
  const previous = attempted.get(videoId);
  return previous === undefined || Date.now() - previous >= retryAfterMs;
}

export function prefetchTrackMetadata(tracks: Track[]) {
  for (const track of tracks) {
    if (!canLookup(track)) continue;
    const existing = pending.get(track.videoId);
    if (existing) existing.prefetched = true;
    else pending.set(track.videoId, { track, consumers: 0, prefetched: true });
  }
  startPendingLookups();
}

export function loadTrackMetadata(track: Track) {
  if (!canLookup(track)) return;
  const { videoId } = track;
  const request = pending.get(videoId) ?? { track, consumers: 0, prefetched: false };
  request.consumers += 1;
  pending.set(videoId, request);
  startPendingLookups();
  return () => {
    if (pending.get(videoId) !== request) return;
    request.consumers -= 1;
    if (!request.consumers && !request.prefetched) pending.delete(videoId);
  };
}
