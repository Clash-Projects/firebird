import { playerStore } from '$src/state/player.svelte';
import { settingsStore, type ThemeMode } from '$src/state/settings.svelte';
import { applyWaveThemeToDocument } from '$src/ui/theme/fluentTheme';
import { WaveColors } from '$src/ui/theme/tokens';
import { resolveAccent } from './accent.svelte';
import { ensureWebFontLoaded, fontFamilyFor, isWebFont } from './appFonts.svelte';

export function syncThemeDocument() {
  const accent = resolveAccent();
  const resolvedFont = settingsStore.appFont ? fontFamilyFor(settingsStore.appFont) : '';
  if (resolvedFont && isWebFont(resolvedFont)) {
    void ensureWebFontLoaded(resolvedFont);
  }
  applyWaveThemeToDocument({
    mode: settingsStore.themeMode,
    customBase: settingsStore.customThemeBase,
    accent: accent.color || WaveColors.defaultAccent,
    glass: settingsStore.performanceMode ? false : settingsStore.glassmorphism,
    fontFamily: resolvedFont,
    uiScale: settingsStore.uiScale,
    artworkColor: playerStore.currentTrack?.accentColor ?? null,
    hazeMaterial: settingsStore.performanceMode ? 'solid' : settingsStore.hazeMaterial,
    hazeIntensity: settingsStore.hazeIntensity,
    liquidGlass: settingsStore.performanceMode ? false : settingsStore.liquidGlass,
    performanceMode: settingsStore.performanceMode,
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
