const PALETTE_STYLE_ID = 'lw-active-palette';
const THEME_STYLE_ID = 'lw-active-theme';
const SNIPPETS_STYLE_ID = 'lw-active-snippets';

function setStyleTag(id: string, css: string | null) {
  let el = document.getElementById(id) as HTMLStyleElement | null;
  if (!css || !css.trim()) {
    el?.remove();
    return;
  }
  if (!el) {
    el = document.createElement('style');
    el.id = id;
    document.head.appendChild(el);
  }
  el.textContent = css;
}

export function parsePaletteTokens(raw: string | null | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  if (!raw || !raw.trim()) return out;
  const trimmed = raw.trim();
  if (trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed) as Record<string, unknown>;
      for (const [key, val] of Object.entries(parsed)) {
        if (typeof val === 'string' && /^--lw-[a-z0-9-]+$/i.test(key) && !/[;{}<>]/.test(val)) {
          out[key] = val.trim();
        }
      }
    } catch {
      return out;
    }
    return out;
  }
  const regex = /(--lw-[a-z0-9-]+)\s*:\s*([^;{}<>]+);/gi;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(trimmed)) !== null) {
    out[match[1]] = match[2].trim();
  }
  return out;
}

export function compilePaletteContent(raw: string | null | undefined): string | null {
  if (!raw || !raw.trim()) return null;
  const trimmed = raw.trim();
  if (!trimmed.startsWith('{')) {
    return trimmed;
  }
  const parsed = parsePaletteTokens(trimmed);
  const lines = Object.entries(parsed).map(([key, val]) => `  ${key}: ${val};`);
  if (lines.length === 0) return null;
  if (parsed['--lw-surface-raised'] && !parsed['--lw-bg-elevated']) {
    lines.push(`  --lw-bg-elevated: var(--lw-surface-raised);`);
  }
  if (parsed['--lw-bg'] && !parsed['--lw-bg-base']) {
    lines.push(`  --lw-bg-base: var(--lw-bg);`);
  }
  return `:root {\n${lines.join('\n')}\n}`;
}

export function extractPaletteSwatch(raw: string | null | undefined): [string, string, string] | undefined {
  const parsed = parsePaletteTokens(raw);
  const bg = parsed['--lw-bg'] ?? parsed['--lw-bg-base'];
  const surface = parsed['--lw-surface-raised'] ?? parsed['--lw-bg-elevated'] ?? parsed['--lw-surface'];
  const accent = parsed['--lw-accent'];
  if (bg && surface && accent) {
    return [bg, surface, accent];
  }
  return undefined;
}

export function applyPaletteCss(raw: string | null) {
  setStyleTag(PALETTE_STYLE_ID, compilePaletteContent(raw));
}

export function applyThemeCss(css: string | null) {
  setStyleTag(THEME_STYLE_ID, css);
}

export function applySnippetsCss(snippets: string[]) {
  const joined = snippets.filter((s) => s.trim().length > 0).join('\n\n');
  setStyleTag(SNIPPETS_STYLE_ID, joined || null);
}
