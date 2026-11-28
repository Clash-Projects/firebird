import type { ThemeMode } from '$src/state/settings.svelte';
import { isLightBase, onAccentFor, surfacesFromCustomBase, WaveColors, withAlpha } from './tokens';

export interface WaveThemeOptions {
  mode: ThemeMode;
  customBase: string;
  accent: string;
  glass: boolean;
  fontFamily: string;
  uiScale: number;
  artworkColor?: string | null;
  hazeMaterial?: 'automatic' | 'haze' | 'solid';
  hazeIntensity?: 'low' | 'medium' | 'high';
  liquidGlass?: boolean;
  performanceMode?: boolean;
  pluginOverride?: boolean;
  pluginTheme?: boolean;
}

const CUSTOM_SURFACE_PROPS = [
  '--lw-bg',
  '--lw-surface',
  '--lw-surface-raised',
  '--lw-surface-overlay',
  '--lw-panel-translucent',
  '--lw-dock-translucent',
  '--lw-rail-translucent',
  '--lw-outline',
  '--lw-outline-soft',
  '--lw-divider',
] as const;

export function applyWaveThemeToDocument(opts: WaveThemeOptions) {
  const root = document.documentElement;
  const effectiveBase =
    opts.mode === 'artwork' && opts.artworkColor ? opts.artworkColor : opts.customBase;
  const isCustomOrArtwork = opts.mode === 'custom' || (opts.mode === 'artwork' && Boolean(opts.artworkColor));
  const effectiveLight =
    opts.mode === 'light' || (opts.mode === 'custom' && isLightBase(opts.customBase));

  if (opts.pluginOverride) {
    root.dataset.theme = 'dark';
    root.dataset.pluginOverride = 'true';
  } else {
    delete root.dataset.pluginOverride;
    if (effectiveLight) {
      root.dataset.theme = 'light';
    } else if (opts.mode === 'amoled' || opts.mode === 'volt') {
      root.dataset.theme = opts.mode;
    } else {
      root.dataset.theme = 'dark';
    }
  }

  if (opts.pluginTheme) {
    root.dataset.pluginTheme = 'true';
    delete root.dataset.glass;
    delete root.dataset.liquidGlass;
  } else {
    delete root.dataset.pluginTheme;
    if (opts.glass) {
      root.dataset.glass = 'true';
    } else {
      delete root.dataset.glass;
    }
    if (opts.liquidGlass) {
      root.dataset.liquidGlass = 'true';
    } else {
      delete root.dataset.liquidGlass;
    }
  }

  if (opts.hazeMaterial) {
    root.dataset.hazeMaterial = opts.hazeMaterial;
  } else {
    delete root.dataset.hazeMaterial;
  }

  if (opts.hazeIntensity) {
    root.dataset.hazeIntensity = opts.hazeIntensity;
  } else {
    delete root.dataset.hazeIntensity;
  }

  if (opts.performanceMode) {
    root.dataset.performance = 'true';
  } else {
    delete root.dataset.performance;
  }

  if (opts.pluginOverride) {
    // Let active plugin theme/palette CSS win cleanly without inline Appearance tokens bleeding over.
    for (const prop of CUSTOM_SURFACE_PROPS) {
      root.style.removeProperty(prop);
    }
    root.style.removeProperty('--lw-accent');
    root.style.removeProperty('--lw-on-accent');
    root.style.removeProperty('--lw-accent-subtle');
    root.style.removeProperty('--lw-font-family');
    root.style.removeProperty('--lw-font-display');
  } else {
    if (isCustomOrArtwork && !opts.glass) {
      const s = surfacesFromCustomBase(effectiveBase);
      root.style.setProperty('--lw-bg', s.background);
      root.style.setProperty('--lw-surface', s.surface);
      root.style.setProperty('--lw-surface-raised', s.surfaceRaised);
      root.style.setProperty('--lw-surface-overlay', s.surfaceOverlay);
      root.style.setProperty('--lw-panel-translucent', opts.hazeMaterial === 'solid' ? s.surface : s.panelTranslucent);
      root.style.setProperty('--lw-dock-translucent', opts.hazeMaterial === 'solid' ? s.background : s.dockTranslucent);
      root.style.setProperty('--lw-rail-translucent', opts.hazeMaterial === 'solid' ? s.background : s.railTranslucent);
      root.style.setProperty('--lw-outline', s.outline);
      root.style.setProperty('--lw-outline-soft', s.outlineSoft);
      root.style.setProperty('--lw-divider', s.outlineSoft);
    } else {
      for (const prop of CUSTOM_SURFACE_PROPS) {
        root.style.removeProperty(prop);
      }
    }

    const safeAccent = opts.accent || WaveColors.defaultAccent;
    root.style.setProperty('--lw-accent', safeAccent);
    root.style.setProperty('--lw-on-accent', onAccentFor(safeAccent));
    if (/^#[0-9a-fA-F]{3,8}$/.test(safeAccent)) {
      root.style.setProperty('--lw-accent-subtle', withAlpha(safeAccent, effectiveLight ? 0.1 : 0.14));
    } else {
      root.style.setProperty('--lw-accent-subtle', 'color-mix(in srgb, var(--lw-accent) 14%, transparent)');
    }

    if (opts.fontFamily) {
      root.style.setProperty(
        '--lw-font-family',
        `"${opts.fontFamily}", "Segoe UI Variable", system-ui, sans-serif`
      );
      root.style.setProperty('--lw-font-display', 'var(--lw-font-family)');
    } else {
      root.style.removeProperty('--lw-font-family');
      root.style.removeProperty('--lw-font-display');
    }
  }

  root.style.setProperty('--lw-ui-scale', String(opts.uiScale || 1));
}
