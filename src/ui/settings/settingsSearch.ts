import { getPlatformAudioCopy, type HostPlatform } from '$src/features/audio_output/platformAudio';

export interface SettingsSearchEntry {
  section: string;
  label: string;
  keywords: string[];
}

export function getSettingsSearchIndex(platform: HostPlatform = 'windows'): SettingsSearchEntry[] {
  const audioCopy = getPlatformAudioCopy(platform);
  const platformKeywords =
    platform === 'macos'
      ? ['coreaudio', 'hog', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality', 'crossfade', 'automix']
      : platform === 'linux'
        ? ['alsa', 'pipewire', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality', 'crossfade', 'automix']
        : ['wasapi', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality', 'crossfade', 'automix'];

  return [
    { section: 'general', label: 'Language, UI Scale & Diagnostic Logs', keywords: ['language', 'scale', 'zoom', 'logs', 'export', 'diagnostics', 'update'] },
    { section: 'appearance', label: 'Theme, Accent, Glass & Fonts', keywords: ['theme', 'dark', 'amoled', 'volt', 'light', 'glass', 'liquid', 'seekbar', 'font', 'accent'] },
    { section: 'audio', label: `Quality, ${audioCopy.exclusiveTitle}, Loudness & AutoMix`, keywords: platformKeywords },
    { section: 'lyrics', label: 'Lyrics Provider, Karaoke Sync & Romanization', keywords: ['lyrics', 'karaoke', 'provider', 'romanized', 'font'] },
    { section: 'integrations', label: 'YouTube Music, Spotify, Scrobblers & Discord RPC', keywords: ['youtube', 'spotify', 'lastfm', 'listenbrainz', 'librefm', 'scrobble', 'discord', 'rpc'] },
    { section: 'library', label: 'Local Folders, Downloads, Backup & Cache', keywords: ['folder', 'scan', 'download', 'backup', 'cache'] },
    { section: 'about', label: 'Version & Updates', keywords: ['version', 'about', 'update'] },
  ];
}

export function searchSettings(query: string, platform: HostPlatform = 'windows'): SettingsSearchEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return getSettingsSearchIndex(platform).filter(
    (entry) =>
      entry.label.toLowerCase().includes(q) ||
      entry.keywords.some((kw) => kw.includes(q))
  );
}
