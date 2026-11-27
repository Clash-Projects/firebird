import { playerStore } from '$src/state/player.svelte';
import { settingsStore, type ThemeMode } from '$src/state/settings.svelte';
import { applyWaveThemeToDocument } from '$src/ui/theme/fluentTheme';
import { WaveColors } from '$src/ui/theme/tokens';
import { resolveAccent } from './accent.svelte';

export function syncThemeDocument() {
  const accent = resolveAccent();
  applyWaveThemeToDocument({
    mode: settingsStore.themeMode,
    customBase: settingsStore.customThemeBase,
    accent: accent.color || WaveColors.defaultAccent,
    glass: settingsStore.glassmorphism,
    fontFamily: settingsStore.appFont,
    uiScale: settingsStore.uiScale,
    artworkColor: playerStore.currentTrack?.accentColor ?? null,
  });
}

export function setThemeMode(mode: ThemeMode) {
  settingsStore.selectTheme(mode);
  syncThemeDocument();
}

export function setCustomBase(hex: string) {
  settingsStore.selectTheme('custom', hex);
  syncThemeDocument();
}
