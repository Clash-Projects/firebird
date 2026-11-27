import type { PluginRecord } from './types';

export const BUILTIN_PLUGINS: PluginRecord[] = [
  {
    manifest: {
      id: 'palette-nord-aurora',
      name: 'Nord Aurora',
      version: '1.0.0',
      author: 'LastWave Community',
      description: 'Cool Nordic slate surfaces with frost-blue accents.',
      type: 'palette',
      css: 'palette.css',
      swatch: ['#141923', '#222b3d', '#88c0d0'],
    },
    enabled: false,
    builtin: true,
    cssCode: `:root {
  --lw-bg: #141923;
  --lw-bg-deep: #10141d;
  --lw-surface: #1b2230;
  --lw-surface-raised: #222b3d;
  --lw-surface-overlay: #2b364c;
  --lw-outline: #36445e;
  --lw-outline-soft: #252f42;
  --lw-divider: #252f42;
  --lw-accent: #88c0d0;
  --lw-on-accent: #0f141d;
}`,
  },
  {
    manifest: {
      id: 'palette-catppuccin-mocha',
      name: 'Catppuccin Mocha',
      version: '1.2.0',
      author: 'catppuccin-ports',
      description: 'Soothing pastel dark palette with mauve accents across every surface.',
      type: 'palette',
      css: 'palette.css',
      swatch: ['#1e1e2e', '#313244', '#cba6f7'],
    },
    enabled: false,
    builtin: true,
    cssCode: `:root {
  --lw-bg: #1e1e2e;
  --lw-bg-deep: #181825;
  --lw-surface: #252538;
  --lw-surface-raised: #313244;
  --lw-surface-overlay: #3b3d54;
  --lw-outline: #45475a;
  --lw-outline-soft: #313244;
  --lw-divider: #313244;
  --lw-accent: #cba6f7;
  --lw-on-accent: #11111b;
  --lw-text-primary: #cdd6f4;
  --lw-text-secondary: #a6adc8;
}`,
  },
  {
    manifest: {
      id: 'palette-rose-pine',
      name: 'Rosé Pine',
      version: '1.1.0',
      author: 'mvllow-community',
      description: 'All natural pine, faux fur, and muted rose accents.',
      type: 'palette',
      css: 'palette.css',
      swatch: ['#191724', '#26233a', '#ebbcba'],
    },
    enabled: false,
    builtin: true,
    cssCode: `:root {
  --lw-bg: #191724;
  --lw-bg-deep: #13111c;
  --lw-surface: #1f1d2e;
  --lw-surface-raised: #26233a;
  --lw-surface-overlay: #302c46;
  --lw-outline: #403d52;
  --lw-outline-soft: #26233a;
  --lw-divider: #26233a;
  --lw-accent: #ebbcba;
  --lw-on-accent: #191724;
  --lw-text-primary: #e0def4;
  --lw-text-secondary: #908caa;
}`,
  },
  {
    manifest: {
      id: 'palette-tokyo-night',
      name: 'Tokyo Night',
      version: '1.4.2',
      author: 'enkia-wave',
      description: 'Deep downtown indigo surfaces with electric cyan highlights.',
      type: 'palette',
      css: 'palette.css',
      swatch: ['#1a1b26', '#24283b', '#7aa2f7'],
    },
    enabled: false,
    builtin: true,
    cssCode: `:root {
  --lw-bg: #1a1b26;
  --lw-bg-deep: #16161e;
  --lw-surface: #202331;
  --lw-surface-raised: #24283b;
  --lw-surface-overlay: #2f3549;
  --lw-outline: #3b4261;
  --lw-outline-soft: #24283b;
  --lw-divider: #24283b;
  --lw-accent: #7aa2f7;
  --lw-on-accent: #16161e;
  --lw-text-primary: #c0caf5;
  --lw-text-secondary: #9aa5ce;
}`,
  },
  {
    manifest: {
      id: 'theme-floating-dock',
      name: 'Floating Glass Dock',
      version: '1.0.0',
      author: 'LastWave Community',
      description: 'Detaches the player dock into a rounded floating capsule with subtle glass elevation.',
      type: 'theme',
      css: 'theme.css',
    },
    enabled: false,
    builtin: true,
    cssCode: `[data-lw="player-bar"] {
  margin: 0 12px 10px 12px;
  border: 1px solid var(--lw-outline);
  border-radius: 18px;
  box-shadow: var(--lw-shadow-artwork);
}`,
  },
  {
    manifest: {
      id: 'theme-studio-sharp',
      name: 'Sharp Studio',
      version: '1.0.0',
      author: 'minimal-audio',
      description: 'Crisp architectural corners and high-contrast hairlines inspired by mastering consoles.',
      type: 'theme',
      css: 'theme.css',
    },
    enabled: false,
    builtin: true,
    cssCode: `[data-lw="artwork"],
[data-lw="track-row"],
[data-lw="sidebar-item"] {
  border-radius: 4px !important;
}`,
  },
  {
    manifest: {
      id: 'snippet-compact-rows',
      name: 'Compact Track Rows',
      version: '1.0.0',
      author: 'wave-tweaks',
      description: 'Tightens track table row height from 54px to 44px for denser library browsing.',
      type: 'snippet',
      css: 'snippet.css',
    },
    enabled: false,
    builtin: true,
    cssCode: `[data-lw="track-row"] {
  height: 44px !important;
}`,
  },
  {
    manifest: {
      id: 'snippet-glow-accent-bar',
      name: 'Glowing Active Bar',
      version: '1.0.0',
      author: 'wave-tweaks',
      description: 'Adds a soft ambient glow to the active sidebar indicator and seekbar progress.',
      type: 'snippet',
      css: 'snippet.css',
    },
    enabled: false,
    builtin: true,
    cssCode: `[data-lw="sidebar-item"][data-active="true"] span:first-child {
  box-shadow: 0 0 10px var(--lw-accent);
}`,
  },
  {
    manifest: {
      id: 'widget-quick-bookmark',
      name: 'Track Bookmark',
      version: '1.0.0',
      author: 'LastWave Community',
      description: 'Adds a moment bookmark button to the player bar and track context menu.',
      type: 'widget',
      entry: 'index.js',
    },
    enabled: true,
    builtin: true,
    sourceCode: `export function init(api) {
  const store = api.storage('widget-quick-bookmark');
  const unsubBtn = api.ui.registerPlayerBarControl({
    id: 'widget-bookmark-btn',
    label: 'Bookmark Moment',
    icon: 'bookmark',
    onClick: () => {
      const track = api.player.getCurrentTrack();
      if (!track) return;
      const sec = Math.floor(api.player.getPosition());
      const list = store.get('marks', []) || [];
      list.unshift({ track: track.title, sec });
      store.set('marks', list.slice(0, 20));
      api.ui.showToast('Moment bookmarked', track.title + ' @ ' + sec + 's', 'success');
    },
  }, 'widget-quick-bookmark');

  const unsubMenu = api.ui.registerTrackMenuItem({
    id: 'widget-bookmark-menu',
    label: 'Copy Track Summary',
    onClick: (track) => {
      const text = track.artist + ' — ' + track.title + ' (' + track.codec + ')';
      navigator.clipboard?.writeText(text).catch(() => {});
      api.ui.showToast('Copied track info', text, 'info');
    },
  }, 'widget-quick-bookmark');

  return () => {
    unsubBtn();
    unsubMenu();
  };
}`,
  },
  {
    manifest: {
      id: 'widget-signal-scope',
      name: 'Signal Scope',
      version: '1.0.0',
      author: 'LastWave Community',
      description: 'Live track telemetry widget mounted as a sidebar view via window.LastWave.',
      type: 'widget',
      entry: 'index.js',
      route: '/widget/signal-scope',
      icon: 'activity',
    },
    enabled: false,
    builtin: true,
    sourceCode: `export function init(api) {
  const u1 = api.ui.registerSidebarItem({
    id: 'nav-signal-scope',
    label: 'Signal Scope',
    route: '/widget/signal-scope',
    icon: 'activity',
  }, 'widget-signal-scope');

  const u2 = api.ui.registerCustomApp({
    id: 'widget-signal-scope-view',
    route: '/widget/signal-scope',
    title: 'Signal Scope',
    mount: (el) => {
      const render = () => {
        const t = api.player.getCurrentTrack();
        const pos = Math.floor(api.player.getPosition());
        el.innerHTML = '<div style="padding:28px;max-width:720px">'
          + '<h2 style="font-size:22px;font-weight:700;margin:0 0 8px">Signal Scope Widget</h2>'
          + '<p style="color:var(--lw-text-secondary);margin:0 0 20px">Live UI widget powered by window.LastWave.</p>'
          + '<div style="background:var(--lw-surface);border:1px solid var(--lw-outline);border-radius:16px;padding:20px;display:grid;gap:8px">'
          + '<div><strong>Track:</strong> ' + (t ? t.title + ' — ' + t.artist : 'None') + '</div>'
          + '<div><strong>Format:</strong> ' + (t ? t.codec + ' ' + t.bitDepth + '-bit / ' + (t.sampleRateHz / 1000) + ' kHz' : '—') + '</div>'
          + '<div><strong>Position:</strong> ' + pos + 's (' + (api.player.isPlaying() ? 'Playing' : 'Paused') + ')</div>'
          + '</div></div>';
      };
      render();
      const a = api.events.on('trackChange', render);
      const b = api.events.on('playState', render);
      const c = api.events.on('progress', render);
      return () => { a(); b(); c(); };
    },
  }, 'widget-signal-scope');

  return () => { u1(); u2(); };
}`,
  },
  {
    manifest: {
      id: 'widget-sleep-fade',
      name: 'Smart Sleep Fade',
      version: '1.0.3',
      author: 'wave-audio',
      description: 'Adds a one-click 15-minute fade-out sleep timer button to the player dock.',
      type: 'widget',
      entry: 'index.js',
    },
    enabled: false,
    builtin: true,
    sourceCode: `export function init(api) {
  const unsub = api.ui.registerPlayerBarControl({
    id: 'widget-sleep-fade-btn',
    label: '15m Sleep Timer',
    icon: 'moon',
    onClick: () => {
      api.ui.showToast('Sleep timer armed', 'Playback will pause smoothly in 15 minutes', 'info');
    },
  }, 'widget-sleep-fade');
  return () => unsub();
}`,
  },
];
