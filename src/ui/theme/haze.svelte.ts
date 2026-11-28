import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
import { settingsStore } from '$src/state/settings.svelte';

export type LwHazeLevel = 'l0' | 'l1' | 'l2' | 'l3';

export const lwHazeSpecs: Record<LwHazeLevel, { blurSigma: number; darkAlpha: number; lightAlpha: number }> = {
  l0: { blurSigma: 0, darkAlpha: 1.0, lightAlpha: 1.0 },
  l1: { blurSigma: 12, darkAlpha: 0.88, lightAlpha: 0.9 },
  l2: { blurSigma: 20, darkAlpha: 0.82, lightAlpha: 0.88 },
  l3: { blurSigma: 28, darkAlpha: 0.86, lightAlpha: 0.94 },
};

export function resolveHazeBlur(level: LwHazeLevel, enableBlur = true): number {
  if (!enableBlur || level === 'l0') return 0;
  if (!pluginManager.safeMode && !pluginManager.allDisabled && pluginManager.activeThemeId) return 0;
  if (settingsStore.performanceMode || settingsStore.hazeMaterial === 'solid') return 0;
  if (level === 'l1' && settingsStore.hazeIntensity === 'low') return 0;
  const intensityScale =
    settingsStore.hazeIntensity === 'low' ? 0.6 : settingsStore.hazeIntensity === 'high' ? 1.25 : 1.0;
  const liquidScale = settingsStore.liquidGlass ? 1.45 : 1.0;
  return Math.round(lwHazeSpecs[level].blurSigma * intensityScale * liquidScale);
}
