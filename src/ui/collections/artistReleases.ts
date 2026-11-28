import { mapYtAlbum, mapYtTrack } from '$src/state/library.svelte';
import type { Album, Track } from '$src/state/types';

export interface ArtistReleaseShelf {
  title?: string;
  entities?: Parameters<typeof mapYtAlbum>[0][];
  tracks?: Parameters<typeof mapYtTrack>[0][];
  trackCards?: Parameters<typeof mapYtTrack>[0][];
}

export function readArtistReleases(shelves: ArtistReleaseShelf[], artistName: string) {
  const albums = new Map<string, Album>();
  const essentialIds = new Set<string>();
  let featuredAlbumId: string | undefined;
  let featuredTrack: Track | undefined;

  for (const shelf of shelves) {
    const title = shelf.title?.trim() ?? '';
    if (/^(featured on|appears on|related albums)$/i.test(title)) continue;
    const latest = /^(latest|new) releases?$/i.test(title);
    const essential = /^essential albums$/i.test(title);

    for (const entity of shelf.entities ?? []) {
      if (entity.kind !== 'album' || !entity.browseId) continue;

      const metadata = entity.subtitle?.split(/\s*[·•]\s*/) ?? [];
      const type = metadata.find((part) => /^(album|single|ep)$/i.test(part.trim()))?.trim().toLowerCase();
      const previous = albums.get(entity.browseId);
      const album = mapYtAlbum(entity);
      album.artist = entity.artist || previous?.artist || artistName;
      album.artworkUrl ||= previous?.artworkUrl || '';
      album.year = Number(entity.subtitle?.match(/\b(?:19|20)\d{2}\b/)?.[0]) || previous?.year || 0;
      if (type === 'ep' || (!type && /\s[-–]\sEP$/i.test(album.title))) {
        album.type = 'EP';
      } else if (type === 'single' || (!type && (/^singles(?:\s*&\s*EPs)?$/i.test(title) || /\s[-–]\sSingle$/i.test(album.title)))) {
        album.type = 'Single';
      } else if (!type && previous) {
        album.type = previous.type;
      }
      albums.set(album.id, album);
      if (latest) featuredAlbumId ??= album.id;
      if (essential) essentialIds.add(album.id);
    }

    if (latest && !featuredTrack) {
      const song = [...(shelf.tracks ?? []), ...(shelf.trackCards ?? [])].find((track) => track.videoId);
      if (song) featuredTrack = mapYtTrack(song);
    }
  }

  return { albums: [...albums.values()], featuredAlbumId, featuredTrack, essentialIds: [...essentialIds] };
}

export function selectLatestAlbum(albums: Album[]): Album | undefined {
  // Within the same year, prefer singles and EPs. The catalog orders each shelf newest first.
  return [...albums].sort((a, b) => b.year - a.year || Number(a.type === 'Album') - Number(b.type === 'Album'))[0];
}

function artworkKey(url: string): string {
  return url.replace(/=w\d+-h\d+[^/?#]*$/i, '');
}

export function selectEssentialAlbums(albums: Album[], tracks: Track[], featuredIds: string[]): Album[] {
  const featured = featuredIds.flatMap((id) => albums.find((album) => album.id === id) ?? []);
  if (featured.length) return featured;

  const titles = new Set(albums.map((album) => album.title.toLowerCase()));
  const standard = albums.filter((album) => {
    const title = album.title.toLowerCase();
    const base = title.replace(/\s+\([^)]*\b(?:edition|versions?|deluxe|remaster(?:ed)?)\b[^)]*\)$/, '');
    return (base === title || !titles.has(base)) && !/\blive (?:at|from|in)\b|\(dj mix\)/.test(title);
  });

  const candidates = standard.length ? standard : albums;
  const scores = candidates.map((album) => {
    const artwork = artworkKey(album.artworkUrl);
    const score = tracks.slice(0, 20).filter((track) =>
      track.albumId === album.id || track.album.toLowerCase() === album.title.toLowerCase()
      || (artwork && artworkKey(track.artworkUrl) === artwork)
    ).length;
    return { album, score };
  });

  scores.sort((a, b) => b.score - a.score);
  return scores.slice(0, 2).map(({ album }) => album);
}
