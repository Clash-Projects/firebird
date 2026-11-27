import { settingsStore } from '$src/state/settings.svelte';

export const UI_SCALE_OPTIONS = [0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.25] as const;

export function setUiScale(scale: number) {
  settingsStore.uiScale = Math.max(0.8, Math.min(1.3, scale));
  document.documentElement.style.setProperty('--lw-ui-scale', String(settingsStore.uiScale));
}
