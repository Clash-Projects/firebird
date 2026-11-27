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

export function applyPaletteCss(css: string | null) {
  setStyleTag(PALETTE_STYLE_ID, css);
}

export function applyThemeCss(css: string | null) {
  setStyleTag(THEME_STYLE_ID, css);
}

export function applySnippetsCss(snippets: string[]) {
  const joined = snippets.filter((s) => s.trim().length > 0).join('\n\n');
  setStyleTag(SNIPPETS_STYLE_ID, joined || null);
}
