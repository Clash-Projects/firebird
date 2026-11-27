import type { NowPlayingStyle } from '$src/state/types';

export const NOW_PLAYING_STYLES: ReadonlyArray<{
  id: NowPlayingStyle;
  label: string;
  description: string;
}> = [
  { id: 'standard', label: 'Standard', description: 'Cover art on the left, synced lyrics on the right.' },
  { id: 'fullBleed', label: 'Full Bleed', description: 'Edge-to-edge canvas artwork blended into ambient lyrics.' },
  { id: 'minimal', label: 'Minimal', description: 'Sharp square cover art, centered metadata, and clean lyrics.' },
];
