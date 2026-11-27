export const WaveColors = {
  background: '#111410',
  backgroundDeep: '#0D100D',
  surface: '#191E17',
  surfaceRaised: '#222820',
  surfaceOverlay: '#2B3328',
  panelTranslucent: '#191E17',
  dockTranslucent: '#191E17',
  railTranslucent: '#151914',
  outline: '#3B4536',
  outlineSoft: '#2A3226',
  textPrimary: '#F2F5ED',
  textSecondary: '#B5BDAE',
  textTertiary: '#929D8B',
  defaultAccent: '#BFF30C',
  lightAccent: '#486300',
  success: '#7BE3A8',
  warn: '#E0A030',
  danger: '#E0506A',

  lightBackground: '#F3F3F3',
  lightNavBackground: '#ECECEC',
  lightContent: '#F9F9F9',
  lightSurface: '#FFFFFF',
  lightSurfaceRaised: '#FFFFFF',
  lightOverlay: '#EDEBE9',
  lightOutline: '#E1DFDD',
  lightOutlineSoft: '#EDEBE9',
  lightTextPrimary: '#1B1A19',
  lightTextSecondary: '#605E5C',
  lightTextTertiary: '#74726F',

  amoledBackground: '#000000',
  amoledAccent: '#FFFFFF',
  amoledSurface: '#0A0A0A',
  amoledSurfaceRaised: '#111111',
  amoledSurfaceOverlay: '#1A1A1A',
  amoledPanelTranslucent: 'rgba(0, 0, 0, 0.90)',
  amoledDockTranslucent: 'rgba(0, 0, 0, 0.95)',
  amoledRailTranslucent: 'rgba(0, 0, 0, 0.90)',
} as const;

export interface WaveSurfaces {
  background: string;
  surface: string;
  surfaceRaised: string;
  surfaceOverlay: string;
  panelTranslucent: string;
  dockTranslucent: string;
  railTranslucent: string;
  outline: string;
  outlineSoft: string;
  isCustom: boolean;
}

export const CUSTOM_THEME_PRESETS: ReadonlyArray<{ name: string; color: string }> = [
  { name: 'Ocean', color: '#1A2A44' },
  { name: 'Forest', color: '#13261C' },
  { name: 'Plum', color: '#2A1838' },
  { name: 'Wine', color: '#3A1622' },
  { name: 'Espresso', color: '#2B2018' },
  { name: 'Slate', color: '#1E242C' },
  { name: 'Teal', color: '#0B3D3A' },
  { name: 'Sand', color: '#EFE7DA' },
  { name: 'Mint', color: '#E4F1E9' },
  { name: 'Lavender', color: '#ECE6F6' },
  { name: 'Sky', color: '#E3ECF7' },
];

function hexToRgb(hex: string): [number, number, number] {
  const clean = hex.replace('#', '');
  const full = clean.length === 3 ? clean.split('').map((c) => c + c).join('') : clean;
  const n = parseInt(full.slice(0, 6), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function rgbToHex(r: number, g: number, b: number): string {
  const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
  return '#' + [clamp(r), clamp(g), clamp(b)].map((v) => v.toString(16).padStart(2, '0')).join('');
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (max === rn) h = (gn - bn) / d + (gn < bn ? 6 : 0);
  else if (max === gn) h = (bn - rn) / d + 2;
  else h = (rn - gn) / d + 4;
  return [h * 60, s, l];
}

function hslToHex(h: number, s: number, l: number): string {
  const sc = Math.max(0, Math.min(1, s));
  const lc = Math.max(0, Math.min(1, l));
  const c = (1 - Math.abs(2 * lc - 1)) * sc;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = lc - c / 2;
  let r = 0, g = 0, b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return rgbToHex((r + m) * 255, (g + m) * 255, (b + m) * 255);
}

export function relativeLuminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

export function isLightBase(hex: string): boolean {
  return relativeLuminance(hex) > 0.35;
}

export function onAccentFor(hex: string): string {
  return contrastRatio(hex, '#000000') >= contrastRatio(hex, '#FFFFFF') ? '#000000' : '#FFFFFF';
}

function withAlpha(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

export function surfacesFromCustomBase(baseHex: string): WaveSurfaces {
  const [r, g, b] = hexToRgb(baseHex);
  const [h, rawS, rawL] = rgbToHsl(r, g, b);

  if (isLightBase(baseHex)) {
    const sat = Math.min(0.45, Math.max(0, rawS));
    let l = Math.min(0.96, Math.max(0.88, rawL));
    for (let i = 0; i < 40 && contrastRatio(WaveColors.lightTextSecondary, hslToHex(h, sat, l - 0.04)) < 4.5; i++) {
      l += 0.005;
    }
    const card = hslToHex(h, sat, Math.min(l + 0.035, 0.99));
    return {
      background: hslToHex(h, sat, l),
      surface: card,
      surfaceRaised: card,
      surfaceOverlay: hslToHex(h, sat, l - 0.04),
      panelTranslucent: card,
      dockTranslucent: card,
      railTranslucent: hslToHex(h, sat, l - 0.025),
      outline: hslToHex(h, sat * 0.7, l - 0.1),
      outlineSoft: hslToHex(h, sat * 0.7, l - 0.06),
      isCustom: true,
    };
  }

  const sat = Math.min(0.55, Math.max(0, rawS));
  let l = Math.min(0.16, Math.max(0.04, rawL));
  for (let i = 0; i < 40 && l > 0 && contrastRatio(WaveColors.textSecondary, hslToHex(h, sat * 0.8, l + 0.105)) < 4.5; i++) {
    l -= 0.005;
  }
  const background = hslToHex(h, sat, l);
  const surface = hslToHex(h, sat * 0.9, l + 0.035);
  return {
    background,
    surface,
    surfaceRaised: hslToHex(h, sat * 0.85, l + 0.07),
    surfaceOverlay: hslToHex(h, sat * 0.8, l + 0.105),
    panelTranslucent: withAlpha(surface, 0.9),
    dockTranslucent: withAlpha(hslToHex(h, sat * 0.95, l + 0.0175), 0.95),
    railTranslucent: withAlpha(background, 0.9),
    outline: hslToHex(h, sat * 0.6, l + 0.1),
    outlineSoft: hslToHex(h, sat * 0.6, l + 0.06),
    isCustom: true,
  };
}

export const WaveSpacing = {
  x2: 4,
  x4: 8,
  x8: 12,
  x12: 16,
  x16: 20,
  x20: 24,
  x28: 32,
  x32: 40,
} as const;

export const WaveRadius = {
  tiny: 6,
  controls: 10,
  artwork: 12,
  menu: 14,
  floating: 20,
  hero: 24,
  structural: 0,
} as const;

export const WaveDensity = {
  railCollapsed: 60,
  railExpanded: 200,
  titleBar: 44,
  dock: 84,
  trackRow: 54,
  trackArt: 40,
  contextPanel: 360,
  contextPanelWide: 380,
  contentMax: 1280,
  lyricMax: 620,
  hitArea: 36,
  iconPrimary: 18,
  iconSecondary: 15,
} as const;

export const WaveMotion = {
  fastMs: 110,
  normalMs: 180,
  slowMs: 260,
  easing: 'cubic-bezier(0.23, 1, 0.32, 1)',
} as const;

export const WaveState = {
  hoverAlpha: 0.07,
  pressedAlpha: 0.12,
  focusRing: 2,
  tooltipDelayMs: 400,
} as const;

export type WaveBreakpoint = 'compact' | 'normal' | 'expanded' | 'ultrawide';

export function waveBreakpointFor(width: number): WaveBreakpoint {
  if (width >= 1700) return 'ultrawide';
  if (width >= 1300) return 'expanded';
  if (width >= 900) return 'normal';
  return 'compact';
}
