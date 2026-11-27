import type { NowPlayingStyle } from './types';

export type ThemeMode = 'dark' | 'light' | 'amoled' | 'custom' | 'artwork';
export type AccentSource = 'custom' | 'lastwave' | 'system' | 'artwork';
export type HazeMaterial = 'automatic' | 'haze' | 'solid';
export type HazeIntensity = 'low' | 'medium' | 'high';
export type AmbientStyle = 'aurora' | 'wash' | 'mesh' | 'off';

class SettingsStore {
  themeMode = $state<ThemeMode>('dark');
  amoled = $state(false);
  customThemeBase = $state('#1A2A44');
  accentColor = $state('#F5F4F0');
  accentSource = $state<AccentSource>('lastwave');
  hazeMaterial = $state<HazeMaterial>('automatic');
  hazeIntensity = $state<HazeIntensity>('medium');
  liquidGlass = $state(false);
  glassmorphism = $state(false);
  wavySeekbar = $state(false);
  ambientStyle = $state<AmbientStyle>('aurora');
  nowPlayingStyle = $state<NowPlayingStyle>('standard');
  nowPlayingAutoHideSec = $state(0);
  appFont = $state('');
  uiScale = $state(1.0);
  language = $state('en');
  performanceMode = $state(false);
  reduceMotion = $state(false);

  qualityTier = $state<'auto' | 'high' | 'cdFlac' | 'hiRes96' | 'hiRes192' | 'atmos'>('hiRes192');
  loudnessMode = $state<'off' | 'track' | 'album' | 'lufs'>('track');
  exclusiveMode = $state(false);
  bitPerfectEnabled = $state(true);
  gaplessPlayback = $state(true);
  discordRpcEnabled = $state(true);
  smtcEnabled = $state(true);
  scrobbleEnabled = $state(true);
  listenBrainzConnected = $state(false);
  libreFmConnected = $state(false);

  ytConnected = $state(false);
  ytAccountName = $state('');
  ytAccountEmail = $state('');
  spotifyConnected = $state(false);
  spotifyUser = $state('');
  lastfmConnected = $state(false);
  lastfmUser = $state('');

  downloadFolderTemplate = $state('{artist}/{album}/{track_num} - {title}');
  embedLyrics = $state(true);
  embedArtwork = $state(true);

  homeShelves = $state([
    { id: 'quick_picks', label: 'Quick Picks', visible: true },
    { id: 'recent_plays', label: 'Jump Back In', visible: true },
    { id: 'albums', label: 'New & Notable Albums', visible: true },
    { id: 'personal_mix', label: 'Personal Mixes', visible: true },
    { id: 'spotify_home', label: 'Spotify Picks', visible: true },
    { id: 'genres', label: 'Moods & Genres', visible: true },
  ]);

  moveShelf(from: number, to: number) {
    if (to < 0 || to >= this.homeShelves.length) return;
    const copy = [...this.homeShelves];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    this.homeShelves = copy;
  }

  toggleShelf(id: string) {
    const shelf = this.homeShelves.find((s) => s.id === id);
    if (shelf) shelf.visible = !shelf.visible;
  }
}

export const settingsStore = new SettingsStore();
