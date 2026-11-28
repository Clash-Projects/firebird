import { router } from '$src/app/router.svelte';
import { libraryStore } from '$src/state/library.svelte';

export function goHome() {
  router.navigate('/home');
}

export function goAlbum(id: string) {
  router.navigate(`/album/${id}`);
}

export function goArtist(id: string) {
  router.navigate(`/artist/${id}`);
}

export function goArtistByName(name: string) {
  const clean = name.trim();
  if (!clean) return;
  const lower = clean.toLowerCase();
  const match =
    libraryStore.artists.find((a) => a.name.toLowerCase() === lower) ??
    libraryStore.artists.find((a) => a.name.toLowerCase().includes(lower));
  goArtist(match?.id ?? `name:${encodeURIComponent(clean)}`);
}

export function goArtistPopularTracks(id: string) {
  router.navigate(`/artist/${id}/popular-tracks`);
}

export function goPlaylist(id: string, source: 'local' | 'youtube' | 'spotify' | 'generated' = 'local') {
  if (source === 'youtube') router.navigate(`/yt-playlist/${id}`);
  else if (source === 'spotify') router.navigate(`/spotify-playlist/${id}`);
  else router.navigate(`/playlist/${id}`);
}

export function goGenre(id: string) {
  router.navigate(`/genre/${id}`);
}

export function goSettings(section?: string) {
  router.navigate(section ? `/settings/${section}` : '/settings');
}

export function goNowPlaying() {
  router.navigate('/now-playing');
}
