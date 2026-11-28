import { playTrackNow } from '$src/app/trackActions';
import { ipcInvoke } from '$src/lib/ipc';
import { libraryStore, mapYtAlbum, mapYtArtist, mapYtTrack } from '$src/state/library.svelte';
import type { Album, Artist, SearchResult, Track } from '$src/state/types';
import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';

export interface SearchResults {
  tracks: Track[];
  albums: Album[];
  artists: Artist[];
}

type RankedMatch = SearchResult & { score: number };

const exactNameScore = 4;
const exactArtistScore = exactNameScore + 1;

function searchText(value: string): string {
  return value.normalize('NFKD')
    .toLowerCase()
    .replace(/(\p{Script=Latin})\p{M}+/gu, '$1')
    .replace(/['’]/g, '')
    .replace(/[^\p{L}\p{N}\p{M}]+/gu, ' ')
    .trim();
}

function rank(query: string, label: string, exactScore: number, prefixScore = 2): number {
  if (label === query) return exactScore;
  if (label.startsWith(query)) return prefixScore;
  return label.includes(query) ? 1 : 0;
}

function trackScore(query: string, track: Track): number {
  const title = searchText(track.title);
  const artist = searchText(track.artist);
  if (query === `${title} ${artist}` || query === `${artist} ${title}`) return exactArtistScore + 1;
  return rank(query, title, exactNameScore);
}

function rankResults(query: string, results: SearchResults): RankedMatch[] {
  return [
    ...results.tracks.map((item) => ({ kind: 'track' as const, item, score: trackScore(query, item) })),
    ...results.artists.map((item) => ({ kind: 'artist' as const, item, score: rank(query, searchText(item.name), exactArtistScore, 2.5) })),
    ...results.albums.map((item) => ({ kind: 'album' as const, item, score: rank(query, searchText(item.title), exactNameScore - 1) })),
  ];
}

export function searchMatches(input: string, remote?: SearchResults): RankedMatch[] {
  const query = searchText(input);
  if (!query) return [];

  const local = rankResults(query, libraryStore).filter((match) => {
    if (match.score > 0) return true;
    if (match.kind === 'artist') return false;
    return searchText(match.item.artist).includes(query) ||
      (match.kind === 'track' && searchText(match.item.album).includes(query));
  });
  const online = remote ? rankResults(query, remote) : [];
  const onlineKinds = new Set(online.map((match) => match.kind));
  const seen = new Set<string>();
  return [
    ...online,
    ...local.filter((match) => !onlineKinds.has(match.kind) || match.score >= exactNameScore),
  ].filter((match) => {
    const key = `${match.kind}:${match.item.id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }).sort((a, b) => b.score - a.score);
}

export function exactArtist(matches: RankedMatch[]): Artist | undefined {
  const [first, second] = matches;
  return first?.kind === 'artist' && first.score === exactArtistScore && second?.score !== first.score
    ? first.item
    : undefined;
}

export async function fetchSearchResults(query: string, limits: Record<keyof SearchResults, number>): Promise<SearchResults> {
  const [tracks, albums, artists] = await Promise.all([
    ipcInvoke<Parameters<typeof mapYtTrack>[0][]>('yt_search_songs', { query, limit: limits.tracks }),
    ipcInvoke<Parameters<typeof mapYtAlbum>[0][]>('yt_search_albums', { query, limit: limits.albums }),
    ipcInvoke<Parameters<typeof mapYtArtist>[0][]>('yt_search_artists', { query, limit: limits.artists }),
  ]);
  return {
    tracks: Array.isArray(tracks) ? tracks.map(mapYtTrack) : [],
    albums: Array.isArray(albums) ? albums.map(mapYtAlbum) : [],
    artists: Array.isArray(artists) ? artists.filter((artist) => artist.browseId).map(mapYtArtist) : [],
  };
}

export function rememberSearchResults(results: SearchResults) {
  libraryStore.upsertTracks(results.tracks);
  libraryStore.upsertAlbums(results.albums);
  libraryStore.upsertArtists(results.artists);
}

export function searchResultDetails(result: SearchResult) {
  if (result.kind === 'artist') {
    return { label: result.item.name, sub: 'Artist', art: result.item.artworkUrl };
  }
  const { title, artist, artworkUrl } = result.item;
  const type = result.kind === 'track' ? 'Song' : 'Album';
  return { label: title, sub: artist ? `${type} · ${artist}` : type, art: artworkUrl };
}

export function openSearchResult(result: SearchResult, query = '', tracks?: Track[]) {
  const { item } = result;
  // Rebuild the entry here so a ranked result does not persist a query-specific score.
  if (result.kind === 'track') {
    void libraryStore.recordSearchResult({ kind: 'track', item: result.item }, query);
    playTrackNow(result.item, tracks);
  } else if (result.kind === 'album') {
    void libraryStore.recordSearchResult({ kind: 'album', item: result.item }, query);
    if (!libraryStore.albums.some((album) => album.id === item.id)) libraryStore.upsertAlbums([result.item]);
    goAlbum(item.id);
  } else {
    void libraryStore.recordSearchResult({ kind: 'artist', item: result.item }, query);
    if (!libraryStore.artists.some((artist) => artist.id === item.id)) libraryStore.upsertArtists([result.item]);
    goArtist(item.id);
  }
}
