import type { NowPlayingStyle } from '$src/state/types';

export const NOW_PLAYING_STYLES: ReadonlyArray<{
  id: NowPlayingStyle;
  label: string;
  description: string;
}> = [
  { id: 'standard', label: 'Studio Split', description: 'Artwork and controls on the left, live karaoke lyrics on the right.' },
  { id: 'minimal', label: 'Minimal Focus', description: 'Centered floating cover art with quiet transport controls.' },
  { id: 'fullBleed', label: 'Full Bleed', description: 'Edge-to-edge blurred canvas stage with bottom glass overlay.' },
];
