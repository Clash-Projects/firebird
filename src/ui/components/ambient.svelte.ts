import { playerStore } from '$src/state/player.svelte';
import { settingsStore } from '$src/state/settings.svelte';

export function activeAmbientColor(): string | null {
  if (settingsStore.ambientStyle === 'off') return null;
  return playerStore.currentTrack?.accentColor ?? null;
}
