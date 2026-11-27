import { router } from '$src/app/router.svelte';

export function goHome() {
  router.navigate('/home');
}

export function goAlbum(id: string) {
  router.navigate(`/album/${id}`);
}

export function goArtist(id: string) {
  router.navigate(`/artist/${id}`);
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
