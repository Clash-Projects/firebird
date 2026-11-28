import { getPlatformAudioCopy, type HostPlatform } from '$src/features/audio_output/platformAudio';

export interface SettingsSearchEntry {
  id: string;
  section: string;
  title: string;
  subtitle: string;
  keywords: string;
}

export function getSettingsSearchIndex(platform: HostPlatform = 'windows'): SettingsSearchEntry[] {
  const audioCopy = getPlatformAudioCopy(platform);
  return [
    {
      id: 'general.behaviour',
      section: 'general',
      title: 'Behaviour',
      subtitle: 'Startup and window defaults',
      keywords:
        'close button minimize tray background window startup miniplayer mini player style compact card ambient classic lyrics',
    },
    {
      id: 'general.language',
      section: 'general',
      title: 'Language',
      subtitle: 'Select the interface and InnerTube language',
      keywords:
        'language locale english hindi spanish french german russian turkish chinese japanese korean arabic vietnamese portuguese indonesian italian translation l10n',
    },
    {
      id: 'general.backup',
      section: 'general',
      title: 'Backup',
      subtitle: 'Move your playlists and settings to another device',
      keywords: 'export import json restore backup playlists migrate',
    },
    {
      id: 'general.cache',
      section: 'general',
      title: 'Cached data',
      subtitle: 'Pages saved on this device so they open instantly',
      keywords: 'cache clear storage snapshot disk space lyrics home offline',
    },
    {
      id: 'general.diagnostics',
      section: 'general',
      title: 'Diagnostics',
      subtitle: 'Local logs for troubleshooting',
      keywords: 'logs verbose logging export log folder crash report debug',
    },
    {
      id: 'integrations.ytm',
      section: 'integrations',
      title: 'YouTube Music',
      subtitle: 'Personal library, history and uploads',
      keywords:
        'youtube music ytm google account sign in cookies brand channel sync listening history liked songs',
    },
    {
      id: 'integrations.spotify',
      section: 'integrations',
      title: 'Spotify',
      subtitle: 'Your playlists, Liked Songs and Canvas',
      keywords:
        'spotify sp_dc cookie sign in login webview canvas playlists liked songs recommendations algorithm radio autoplay',
    },
    {
      id: 'integrations.discord',
      section: 'integrations',
      title: 'Discord',
      subtitle: 'Show what you are listening to',
      keywords: 'discord rich presence status activity rpc',
    },
    {
      id: 'playback.main',
      section: 'playback',
      title: 'Playback',
      subtitle: 'Gapless, crossfade and output behaviour',
      keywords:
        'autoplay similar crossfade gapless radio queue ends canvas animated artwork media controls smtc lock screen',
    },
    {
      id: 'audio.output',
      section: 'audio',
      title: 'Audio output',
      subtitle: `Device, ${audioCopy.exclusiveTitle} and bit-perfect`,
      keywords: 'wasapi coreaudio alsa pipewire exclusive bit-perfect bit perfect device dac output',
    },
    {
      id: 'audio.quality',
      section: 'audio',
      title: 'Streaming and Download quality',
      subtitle: 'Lossless-first with YouTube fallback',
      keywords:
        'lossless hi-res flac atmos opus mp3 prefer lossless streaming download quality bitrate data saver',
    },
    {
      id: 'audio.loudness',
      section: 'audio',
      title: 'Loudness',
      subtitle: 'Track or album normalization',
      keywords: 'loudness replaygain normalization volume lufs',
    },
    {
      id: 'audio.headphones',
      section: 'audio',
      title: 'Headphone correction',
      subtitle: 'Built-in curves for common headphones',
      keywords: 'headphone autoeq eq correction sony bose airpods sennheiser',
    },
    {
      id: 'audio.automix',
      section: 'audio',
      title: 'AutoMix',
      subtitle: 'Timed DJ blends between tracks',
      keywords: 'automix timed mix bars bass swap transition overlap dj',
    },
    {
      id: 'audio.equalizer',
      section: 'audio',
      title: 'Equalizer & Sound FX',
      subtitle: '10-band ISO curves',
      keywords: 'eq equalizer studio master clarity preset bands sound fx preamp',
    },
    {
      id: 'downloads.offline',
      section: 'downloads',
      title: 'Offline',
      subtitle: 'Download behaviour, save location and folder layout',
      keywords: 'download lyrics lrc save location folder layout artist album year reorganize directory',
    },
    {
      id: 'library.folders',
      section: 'library',
      title: 'Local folders',
      subtitle: 'Music folders scanned on your computer',
      keywords: 'local music folders scan flac mp3 directory disk',
    },
    {
      id: 'library.sync',
      section: 'library',
      title: 'Library',
      subtitle: 'YouTube Music sync, podcasts and hidden tracks',
      keywords: 'sync youtube music playlists liked songs podcasts hidden exclusions restore sync now',
    },
    {
      id: 'lyrics.animation',
      section: 'lyrics',
      title: 'Lyrics animation',
      subtitle: 'Visual animation style for synced lyrics',
      keywords: 'lyrics animation style apple fluid glow bounce pulse smooth fade karaoke neon glitch',
    },
    {
      id: 'lyrics.timing',
      section: 'lyrics',
      title: 'Timing',
      subtitle: 'Synced lyrics providers',
      keywords: 'word by word karaoke synced lyrics provider source romanisation transliteration',
    },
    {
      id: 'appearance.theme',
      section: 'appearance',
      title: 'Theme',
      subtitle: 'Midnight, Pearl, artwork, or your own colours',
      keywords:
        'dark light midnight pearl custom artwork cover theme background base tint glass glassmorphism frosted translucent blur accent colour color picker palette mode personalize',
    },
    {
      id: 'appearance.haze',
      section: 'appearance',
      title: 'Haze material',
      subtitle: 'Blur and translucency of panels',
      keywords: 'haze blur acrylic mica glass transparency intensity solid',
    },
    {
      id: 'appearance.now_playing_style',
      section: 'appearance',
      title: 'Now Playing style',
      subtitle: 'Layout and presentation for the player screen',
      keywords: 'now playing style layout standard full bleed minimal old edge to edge duet lyrics cover artwork',
    },
    {
      id: 'appearance.now_playing_autohide',
      section: 'appearance',
      title: 'Auto-hide Now Playing controls',
      subtitle: 'Fade out the top buttons after mouse inactivity',
      keywords: 'auto-hide autohide now playing controls fade buttons inactivity seconds timeout mouse',
    },
    {
      id: 'appearance.ambient',
      section: 'appearance',
      title: 'Now Playing ambient',
      subtitle: 'Background behind the cover and lyrics',
      keywords: 'ambient apple music soft background visualizer blur aura stage now playing lyrics colours colors',
    },
    {
      id: 'appearance.font',
      section: 'appearance',
      title: 'Font',
      subtitle: 'The typeface used across the app',
      keywords: 'font typeface typography text inter poppins geist manrope segoe',
    },
    {
      id: 'appearance.scale',
      section: 'appearance',
      title: 'Interface scale',
      subtitle: 'Adjust app zoom for display resolution or preference (Ctrl +/-)',
      keywords: 'zoom scale interface size display resolution dpi ctrl+ ctrl- magnify window',
    },
    {
      id: 'appearance.surfaces',
      section: 'appearance',
      title: 'Surfaces',
      subtitle: 'Contrast and artwork tinting',
      keywords: 'amoled black dynamic artwork theme tint now playing contrast reduce motion',
    },
    {
      id: 'lastfm.scrobbling',
      section: 'lastfm',
      title: 'Scrobbler rules',
      subtitle: 'Thresholds apply across all connected scrobblers',
      keywords: 'scrobble scrobbler now playing threshold percent history',
    },
    {
      id: 'lastfm.connection',
      section: 'lastfm',
      title: 'Scrobbler accounts',
      subtitle: 'Connect Last.fm, ListenBrainz, or Libre.fm',
      keywords: 'lastfm last.fm api key secret listenbrainz libre.fm librefm gnu fm maloja koito scrobble token',
    },
    {
      id: 'sources.addons',
      section: 'sources',
      title: 'Addon sources',
      subtitle: 'Lossless catalog addons',
      keywords: 'addon url source lossless catalog provider server enable disable toggle on off',
    },
    {
      id: 'experimental.main',
      section: 'experimental',
      title: 'Experimental',
      subtitle: 'Opt-in desktop prototypes',
      keywords: 'extra translucency liquid glass wavy seekbar timeline prototype beta',
    },
    {
      id: 'about.main',
      section: 'about',
      title: 'About LastWave',
      subtitle: 'Build and project links',
      keywords: 'version update check updates github links telegram discord issue',
    },
  ];
}

export function sectionDisplayLabel(id: string): string {
  switch (id) {
    case 'lastfm':
      return 'Scrobbler';
    default:
      return id ? `${id[0].toUpperCase()}${id.slice(1)}` : 'Settings';
  }
}

export function searchSettings(
  query: string,
  platform: HostPlatform = 'windows',
  limit = 8
): SettingsSearchEntry[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return [];

  const entries = getSettingsSearchIndex(platform);
  const scored: Array<{ score: number; entry: SettingsSearchEntry }> = [];

  for (const entry of entries) {
    const title = entry.title.toLowerCase();
    const sub = entry.subtitle.toLowerCase();
    const hay = `${title} ${sub} ${entry.keywords} ${entry.section}`.toLowerCase();
    if (!words.every((w) => hay.includes(w))) continue;
    const inTitle = words.every((w) => title.includes(w));
    const inSub = words.every((w) => sub.includes(w));
    scored.push({ score: inTitle ? 0 : inSub ? 1 : 2, entry });
  }

  scored.sort((a, b) => a.score - b.score);
  return scored.slice(0, limit).map((s) => s.entry);
}
