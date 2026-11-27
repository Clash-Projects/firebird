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
      ? ['coreaudio', 'hog', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality']
      : platform === 'linux'
        ? ['alsa', 'pipewire', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality']
        : ['wasapi', 'exclusive', 'bit-perfect', 'dac', 'replaygain', 'lufs', 'quality'];

  return [
    { section: 'general', label: 'Language & UI Scale', keywords: ['language', 'scale', 'zoom', 'performance'] },
    { section: 'integrations', label: 'YouTube Music, Spotify & Discord RPC', keywords: ['youtube', 'spotify', 'discord', 'rpc'] },
    { section: 'playback', label: 'Crossfade, AutoMix & Gapless', keywords: ['crossfade', 'automix', 'gapless', 'speed', 'silence'] },
    { section: 'audio', label: `Bit-Perfect, ${audioCopy.exclusiveTitle} & Loudness`, keywords: platformKeywords },
    { section: 'downloads', label: 'Offline Folder Layout & Metadata', keywords: ['download', 'folder', 'lrc', 'tags'] },
    { section: 'library', label: 'Local Music Folders & Backup', keywords: ['folder', 'scan', 'backup', 'cache'] },
    { section: 'lyrics', label: 'Synced Lyrics & Translation', keywords: ['lyrics', 'karaoke', 'translation', 'romanized'] },
    { section: 'appearance', label: 'Theme, Accent, Haze, Glass & Fonts', keywords: ['theme', 'dark', 'amoled', 'light', 'glass', 'haze', 'font', 'accent'] },
    { section: 'plugins', label: 'Community Themes, Extensions & Custom Apps', keywords: ['plugin', 'theme.css', 'extension', 'custom app', 'marketplace'] },
    { section: 'lastfm', label: 'Last.fm, ListenBrainz & Libre.fm', keywords: ['scrobble', 'lastfm', 'listenbrainz', 'librefm'] },
    { section: 'experimental', label: 'Liquid Glass & Wavy Seekbar', keywords: ['liquid', 'wavy', 'seekbar', 'experimental'] },
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
