export interface AppFont {
  id: string;
  label: string;
  web: boolean;
}

export const APP_FONTS: AppFont[] = [
  { id: '', label: 'System default (Segoe UI Variable)', web: false },
  { id: 'Segoe UI', label: 'Segoe UI', web: false },
  { id: 'Bahnschrift', label: 'Bahnschrift', web: false },
  { id: 'Inter', label: 'Inter', web: true },
  { id: 'Geist', label: 'Geist', web: true },
  { id: 'Manrope', label: 'Manrope', web: true },
  { id: 'Plus Jakarta Sans', label: 'Plus Jakarta Sans', web: true },
  { id: 'DM Sans', label: 'DM Sans', web: true },
  { id: 'Figtree', label: 'Figtree', web: true },
  { id: 'Outfit', label: 'Outfit', web: true },
  { id: 'Onest', label: 'Onest', web: true },
  { id: 'Poppins', label: 'Poppins', web: true },
  { id: 'Sora', label: 'Sora', web: true },
  { id: 'Urbanist', label: 'Urbanist', web: true },
  { id: 'Lexend', label: 'Lexend', web: true },
  { id: 'Be Vietnam Pro', label: 'Be Vietnam Pro', web: true },
  { id: 'Space Grotesk', label: 'Space Grotesk', web: true },
  { id: 'Red Hat Display', label: 'Red Hat Display', web: true },
  { id: 'Rubik', label: 'Rubik', web: true },
  { id: 'Work Sans', label: 'Work Sans', web: true },
  { id: 'IBM Plex Sans', label: 'IBM Plex Sans', web: true },
  { id: 'Montserrat', label: 'Montserrat', web: true },
  { id: 'Nunito', label: 'Nunito', web: true },
  { id: 'Quicksand', label: 'Quicksand', web: true },
  { id: 'Raleway', label: 'Raleway', web: true },
  { id: 'Josefin Sans', label: 'Josefin Sans', web: true },
  { id: 'Open Sans', label: 'Open Sans', web: true },
  { id: 'Roboto', label: 'Roboto', web: true },
  { id: 'Lato', label: 'Lato', web: true },
];

const WEIGHTS = [400, 500, 600, 700] as const;
const loadedFonts = new Map<string, Promise<string | null>>();

export function fontSlugFor(idOrName: string): string {
  return idOrName.trim().toLowerCase().replace(/\s+/g, '-');
}

export function fontFamilyFor(idOrName: string): string {
  const trimmed = idOrName.trim();
  if (!trimmed) return '';
  const match = APP_FONTS.find(
    (f) => f.id.toLowerCase() === trimmed.toLowerCase() || fontSlugFor(f.id) === trimmed.toLowerCase()
  );
  return match?.id ?? trimmed;
}

export function isWebFont(idOrName: string): boolean {
  const canonical = fontFamilyFor(idOrName);
  const entry = APP_FONTS.find((f) => f.id.toLowerCase() === canonical.toLowerCase());
  return Boolean(entry?.web);
}

/**
 * Loads real 400-700 font faces from jsDelivr Fontsource CDN into `document.fonts`
 * so selecting a typography preset actually renders in that font (matching OG app_fonts.dart).
 */
export function ensureWebFontLoaded(idOrName: string): Promise<string | null> {
  if (typeof document === 'undefined' || !idOrName.trim()) {
    return Promise.resolve(null);
  }
  const family = fontFamilyFor(idOrName);
  if (!isWebFont(family)) return Promise.resolve(family);

  const existing = loadedFonts.get(family);
  if (existing) return existing;

  const slug = fontSlugFor(family);
  const promise = (async () => {
    const loads = WEIGHTS.map(async (weight) => {
      const woff2Url = `https://cdn.jsdelivr.net/fontsource/fonts/${slug}@latest/latin-${weight}-normal.woff2`;
      const face = new FontFace(family, `url(${woff2Url}) format("woff2")`, {
        weight: String(weight),
        style: 'normal',
        display: 'swap',
      });
      try {
        const loaded = await face.load();
        document.fonts.add(loaded);
        return true;
      } catch {
        return false;
      }
    });
    const results = await Promise.all(loads);
    return results.some(Boolean) ? family : null;
  })();

  loadedFonts.set(family, promise);
  return promise;
}
