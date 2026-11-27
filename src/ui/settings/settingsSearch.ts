export interface SettingsSearchEntry {
  section: string;
  label: string;
  keywords: string[];
}

export const SETTINGS_SEARCH_INDEX: SettingsSearchEntry[] = [
  { section: 'general', label: 'Language & UI Scale', keywords: ['language', 'scale', 'zoom', 'performance'] },
  { section: 'integrations', label: 'YouTube Music, Spotify & Discord RPC', keywords: ['youtube', 'spotify', 'discord', 'rpc'] },
  { section: 'playback', label: 'Crossfade, AutoMix & Gapless', keywords: ['crossfade', 'automix', 'gapless', 'speed', 'silence'] },
  { section: 'audio', label: 'Bit-Perfect, WASAPI Exclusive & Loudness', keywords: ['wasapi', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality'] },
  { section: 'downloads', label: 'Offline Folder Layout & Metadata', keywords: ['download', 'folder', 'lrc', 'tags'] },
  { section: 'library', label: 'Local Music Folders & Backup', keywords: ['folder', 'scan', 'backup', 'cache'] },
  { section: 'lyrics', label: 'Synced Lyrics & Translation', keywords: ['lyrics', 'karaoke', 'translation', 'romanized'] },
  { section: 'appearance', label: 'Theme, Accent, Haze, Glass & Fonts', keywords: ['theme', 'dark', 'amoled', 'light', 'glass', 'haze', 'font', 'accent'] },
  { section: 'plugins', label: 'Community Themes, Extensions & Custom Apps', keywords: ['plugin', 'spicetify', 'theme.css', 'extension', 'custom app', 'marketplace'] },
  { section: 'lastfm', label: 'Last.fm, ListenBrainz & Libre.fm', keywords: ['scrobble', 'lastfm', 'listenbrainz', 'librefm'] },
  { section: 'experimental', label: 'Liquid Glass & Wavy Seekbar', keywords: ['liquid', 'wavy', 'seekbar', 'experimental'] },
  { section: 'about', label: 'Version, Diagnostics & Updates', keywords: ['version', 'about', 'logs', 'update'] },
];

export function searchSettings(query: string): SettingsSearchEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return SETTINGS_SEARCH_INDEX.filter(
    (entry) =>
      entry.label.toLowerCase().includes(q) ||
      entry.keywords.some((kw) => kw.includes(q))
  );
}
