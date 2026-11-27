import { detectHostPlatform, type HostPlatform } from '$src/features/audio_output/platformAudio';
import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
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
  reduceMotion = $state(false);

  platform = $state<HostPlatform>(detectHostPlatform());
  qualityTier = $state<'auto' | 'high' | 'cdFlac' | 'hiRes96' | 'hiRes192' | 'atmos'>('hiRes192');
  loudnessMode = $state<'off' | 'track' | 'album' | 'lufs'>('track');
  exclusiveMode = $state(false);
  bitPerfectEnabled = $state(false);
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

  async loadFromBackend() {
    if (!isTauriRuntime()) return;
    const [all, lastfm] = await Promise.all([
      ipcInvoke<Record<string, unknown>>('prefs_get_all_settings'),
      ipcInvoke<{ status?: string; username?: string }>('lastfm_auth_state'),
    ]);
    if (all) {
      if (typeof all.themeMode === 'string') {
        const m = all.themeMode as ThemeMode;
        if (m === 'dark' || m === 'light' || m === 'amoled' || m === 'custom' || m === 'artwork') {
          this.themeMode = m;
        }
      }
      if (typeof all.amoled === 'boolean') this.amoled = all.amoled;
      if (typeof all.languageTag === 'string' && all.languageTag) this.language = all.languageTag;
      if (typeof all.uiScale === 'number' && all.uiScale > 0) this.uiScale = all.uiScale;
      if (typeof all.loudnessMode === 'string') {
        const lm = all.loudnessMode as 'off' | 'track' | 'album' | 'lufs';
        if (lm === 'off' || lm === 'track' || lm === 'album' || lm === 'lufs') {
          this.loudnessMode = lm;
        }
      }
      if (typeof all.downloadLyrics === 'boolean') this.embedLyrics = all.downloadLyrics;
      if (typeof all.downloadStructure === 'string' && all.downloadStructure) {
        this.downloadFolderTemplate = all.downloadStructure;
      }
      if (typeof all.losslessQuality === 'number') {
        const q = all.losslessQuality;
        this.qualityTier =
          q >= 28 ? 'atmos' : q >= 27 ? 'hiRes192' : q >= 7 ? 'hiRes96' : q >= 6 ? 'cdFlac' : 'high';
      }
    }
    if (lastfm) {
      this.lastfmConnected = Boolean(lastfm.username) && lastfm.status === 'authenticated';
      this.lastfmUser = lastfm.username || '';
    }
  }

  saveBoolPref(key: string, value: boolean) {
    void ipcInvoke('prefs_set_bool', { key, value });
  }

  saveIntPref(key: string, value: number) {
    void ipcInvoke('prefs_set_int', { key, value: Math.round(value) });
  }

  saveDoublePref(key: string, value: number) {
    void ipcInvoke('prefs_set_double', { key, value });
  }

  saveStringPref(key: string, value: string) {
    void ipcInvoke('prefs_set_string', { key, value });
  }
}

export const settingsStore = new SettingsStore();
