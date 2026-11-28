import { featuresStore } from '$src/state/features.svelte';
import { libraryStore } from '$src/state/library.svelte';
import { playerStore } from '$src/state/player.svelte';
import type { Track } from '$src/state/types';

export function playTrackNow(track: Track, list?: Track[]) {
  playerStore.playTrack(track, list);
}

export function playTrackRadio(track: Track, sourceLabel = 'Radio') {
  playerStore.playRadio(track, sourceLabel);
}

export function playTrackNext(track: Track) {
  playerStore.playNext(track);
  featuresStore.showToast({ title: 'Playing next', subtitle: track.title, severity: 'info' });
}

export function enqueueTrack(track: Track) {
  playerStore.addToQueue(track);
  featuresStore.showToast({ title: 'Added to queue', subtitle: track.title, severity: 'info' });
}

export function toggleTrackLike(track: Track) {
  const wasLiked = libraryStore.isLiked(track.id);
  libraryStore.toggleLike(track.id);
  featuresStore.showToast({
    title: wasLiked ? 'Removed from Liked Songs' : 'Added to Liked Songs',
    subtitle: track.title,
    severity: wasLiked ? 'info' : 'success',
  });
}

export function downloadTrack(track: Track) {
  libraryStore.enqueueDownload(track);
  featuresStore.showToast({
    title: 'Saved for offline playback',
    subtitle: `${track.title} · ${track.codec}`,
    severity: 'success',
  });
}
