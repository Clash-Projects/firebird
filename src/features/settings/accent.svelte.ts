import { playerStore } from '$src/state/player.svelte';
import { settingsStore, type AccentSource } from '$src/state/settings.svelte';
import { WaveColors } from '$src/ui/theme/tokens';
import { readSystemAccent } from './systemAccent';

export interface AccentResolution {
  source: AccentSource;
  color: string;
  resolved: boolean;
}

export function resolveAccent(): AccentResolution {
  const source = settingsStore.accentSource;
  const custom = settingsStore.accentColor;

  if (source === 'lastwave') {
    return { source, color: WaveColors.defaultAccent, resolved: true };
  }
  if (source === 'system') {
    const sys = readSystemAccent();
    return { source, color: sys ?? custom, resolved: Boolean(sys) };
  }
  if (source === 'artwork') {
    const art = playerStore.currentTrack?.accentColor ?? null;
    return { source, color: art ?? custom, resolved: Boolean(art) };
  }
  return { source: 'custom', color: custom, resolved: true };
}
