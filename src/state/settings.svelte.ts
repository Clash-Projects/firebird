import { detectHostPlatform, type HostPlatform } from '$src/features/audio_output/platformAudio';
import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import type { NowPlayingStyle } from './types';

export const THEME_MODES = [
  { value: 'dark', label: 'Midnight' },
  { value: 'amoled', label: 'AMOLED' },
  { value: 'volt', label: 'LastWave Volt' },
  { value: 'light', label: 'Pearl' },
  { value: 'custom', label: 'Custom' },
  { value: 'artwork', label: 'Artwork' },
] as const;

export type ThemeMode = (typeof THEME_MODES)[number]['value'];
export type AccentSource = 'custom' | 'lastwave' | 'system' | 'artwork';
export type HazeMaterial = 'automatic' | 'haze' | 'solid';
export type HazeIntensity = 'low' | 'medium' | 'high';
export type AmbientStyle = 'aurora' | 'wash' | 'mesh' | 'off';
export type MiniPlayerStyle = 'classic' | 'compact' | 'card' | 'ambient' | 'lyrics';

function argbIntToHex(v: unknown): string | null {
  if (typeof v !== 'number' || !Number.isFinite(v)) return null;
  const rgb = (Math.trunc(v) >>> 0) & 0xffffff;
  return `#${rgb.toString(16).padStart(6, '0')}`;
}

class SettingsStore {
  themeMode = $state<ThemeMode>('dark');
  customThemeBase = $state('#1A2A44');
  accentColor = $state('#F5F4F0');
  accentSource = $state<AccentSource>('lastwave');
  hazeMaterial = $state<HazeMaterial>('automatic');
  hazeIntensity = $state<HazeIntensity>('medium');
  liquidGlass = $state(false);
  glassmorphism = $state(false);
  wavySeekbar = $state(false);
  performanceMode = $state(false);
  ambientStyle = $state<AmbientStyle>('aurora');
  nowPlayingStyle = $state<NowPlayingStyle>('standard');
  nowPlayingAutoHideSec = $state(0);
  miniPlayerStyle = $state<MiniPlayerStyle>('classic');
  appFont = $state('');
  uiScale = $state(1.0);
  language = $state('en');
  reduceMotion = $state(false);
  private themeSave: Promise<void> | null = null;

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
  ytAccountAvatar = $state('');
  displayName = $state('');
  jamDisplayName = $state('');
  profilePicturePath = $state('');
  profilePictureUrl = $state('');
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

  selectTheme(mode: ThemeMode, customBase?: string) {
    this.themeMode = mode;
    if (customBase) this.customThemeBase = customBase;

    this.themeSave = (this.themeSave ?? Promise.resolve()).then(async () => {
      if (customBase) {
        const value = 0xff000000 + Number.parseInt(customBase.slice(1), 16);
        await ipcInvoke('prefs_set_int', { key: 'lw_custom_theme_base', value });
      }
      await ipcInvoke('prefs_set_string', { key: 'lw_theme_mode', value: mode });
      await ipcInvoke('prefs_set_bool', { key: 'lw_amoled', value: mode === 'amoled' });
    });
  }

  async loadFromBackend(): Promise<Record<string, unknown> | null> {
    if (!isTauriRuntime()) return null;
    const [all, lastfm] = await Promise.all([
      ipcInvoke<Record<string, unknown>>('prefs_get_all_settings'),
      ipcInvoke<{ status?: string; username?: string }>('lastfm_auth_state'),
    ]);
    if (all) {
      if (typeof all.displayName === 'string') this.displayName = all.displayName;
      if (typeof all.jamDisplayName === 'string') this.jamDisplayName = all.jamDisplayName;
      if (typeof all.profilePicturePath === 'string') {
        this.profilePicturePath = all.profilePicturePath;
      }
      if (typeof all.profilePictureDataUrl === 'string' && all.profilePictureDataUrl) {
        this.profilePictureUrl = all.profilePictureDataUrl;
      }
      if (!this.themeSave) {
        const mode = THEME_MODES.find((theme) => theme.value === all.themeMode)?.value ?? 'dark';
        this.themeMode = mode === 'dark' && all.amoled === true ? 'amoled' : mode;
        const customHex = argbIntToHex(all.customThemeBase);
        if (customHex) this.customThemeBase = customHex;
      }
      const accentHex = argbIntToHex(all.accentColor);
      if (accentHex) this.accentColor = accentHex;
      if (typeof all.accentSource === 'string') {
        const s = all.accentSource as AccentSource;
        if (s === 'custom' || s === 'lastwave' || s === 'system' || s === 'artwork') {
          this.accentSource = s;
        }
      }
      if (typeof all.glassmorphism === 'boolean') this.glassmorphism = all.glassmorphism;
      if (typeof all.liquidGlass === 'boolean') this.liquidGlass = all.liquidGlass;
      if (typeof all.wavySeekbar === 'boolean') this.wavySeekbar = all.wavySeekbar;
      if (typeof all.appFont === 'string') this.appFont = all.appFont;
      if (typeof all.languageTag === 'string' && all.languageTag) this.language = all.languageTag;
      if (typeof all.uiScale === 'number' && all.uiScale > 0) this.uiScale = all.uiScale;
      if (typeof all.nowPlayingStyle === 'string') {
        const nps = all.nowPlayingStyle as NowPlayingStyle;
        if (nps === 'fullBleed' || nps === 'minimal' || nps === 'standard') {
          this.nowPlayingStyle = nps;
        }
      }
      if (typeof all.nowPlayingAutoHideSec === 'number') {
        this.nowPlayingAutoHideSec = all.nowPlayingAutoHideSec;
      }
      if (typeof all.miniPlayerStyle === 'string') {
        const ms = all.miniPlayerStyle as MiniPlayerStyle;
        if (ms === 'classic' || ms === 'compact' || ms === 'card' || ms === 'ambient' || ms === 'lyrics') {
          this.miniPlayerStyle = ms;
        }
      }
      if (typeof all.loudnessMode === 'string') {
        const lm = all.loudnessMode as 'off' | 'track' | 'album' | 'lufs';
        if (lm === 'off' || lm === 'track' || lm === 'album' || lm === 'lufs') {
          this.loudnessMode = lm;
        }
      }
      if (typeof all.wasapiExclusive === 'boolean') this.exclusiveMode = all.wasapiExclusive;
      if (typeof all.bitPerfect === 'boolean') this.bitPerfectEnabled = all.bitPerfect;
      if (typeof all.discordRichPresence === 'boolean') {
        this.discordRpcEnabled = all.discordRichPresence;
      }
      if (typeof all.macosNowPlaying === 'boolean') this.smtcEnabled = all.macosNowPlaying;
      if (typeof all.scrobblerEnabled === 'boolean') this.scrobbleEnabled = all.scrobblerEnabled;
      if (typeof all.listenBrainzConnected === 'boolean') {
        this.listenBrainzConnected = all.listenBrainzConnected;
      }
      if (typeof all.libreFmConnected === 'boolean') {
        this.libreFmConnected = all.libreFmConnected;
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
      this.lastfmConnected = Boolean(lastfm.username) && lastfm.status === 'signedIn';
      this.lastfmUser = lastfm.username || '';
    }
    const [hazeMat, hazeInt, ambStyle, perfMode, reduceMot] = await Promise.all([
      ipcInvoke<string | null>('prefs_get', { key: 'lw_haze_material' }).catch(() => null),
      ipcInvoke<string | null>('prefs_get', { key: 'lw_haze_intensity' }).catch(() => null),
      ipcInvoke<string | null>('prefs_get', { key: 'lw_ambient_style' }).catch(() => null),
      ipcInvoke<boolean | null>('prefs_get', { key: 'lw_performance_mode' }).catch(() => null),
      ipcInvoke<boolean | null>('prefs_get', { key: 'lw_reduce_motion' }).catch(() => null),
    ]);
    if (hazeMat === 'automatic' || hazeMat === 'haze' || hazeMat === 'solid') {
      this.hazeMaterial = hazeMat;
    }
    if (hazeInt === 'low' || hazeInt === 'medium' || hazeInt === 'high') {
      this.hazeIntensity = hazeInt;
    }
    if (ambStyle === 'aurora' || ambStyle === 'wash' || ambStyle === 'mesh' || ambStyle === 'off') {
      this.ambientStyle = ambStyle;
    }
    if (typeof perfMode === 'boolean') this.performanceMode = perfMode;
    if (typeof reduceMot === 'boolean') this.reduceMotion = reduceMot;
    return all ?? null;
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
