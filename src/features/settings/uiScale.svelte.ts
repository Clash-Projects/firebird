import { settingsStore } from '$src/state/settings.svelte';

export const UI_SCALE_OPTIONS = [0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5] as const;

export function setUiScale(scale: number) {
  settingsStore.uiScale = Math.max(0.75, Math.min(1.5, Math.round(scale * 100) / 100));
  document.documentElement.style.setProperty('--lw-ui-scale', String(settingsStore.uiScale));
}
