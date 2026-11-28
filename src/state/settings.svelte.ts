import { detectHostPlatform, type HostPlatform } from '$src/features/audio_output/platformAudio';
import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import type { NowPlayingStyle } from './types';

export const THEME_MODES = [
  { value: 'dark', label: 'Midnight' },
  { value: 'light', label: 'Pearl' },
  { value: 'artwork', label: 'Artwork' },
  { value: 'custom', label: 'Custom' },
  { value: 'amoled', label: 'AMOLED' },
  { value: 'volt', label: 'LastWave Volt' },
] as const;

export type ThemeMode = (typeof THEME_MODES)[number]['value'];
export type AccentSource = 'custom' | 'lastwave' | 'system' | 'artwork';
export type HazeMaterial = 'automatic' | 'haze' | 'solid';
export type HazeIntensity = 'low' | 'medium' | 'high';
export type AmbientStyle = 'apple' | 'soft' | 'aurora' | 'wash' | 'mesh' | 'off';
export type MiniPlayerStyle = 'classic' | 'compact' | 'card' | 'ambient' | 'lyrics';

function argbIntToHex(v: unknown): string | null {
  if (typeof v !== 'number' || !Number.isFinite(v)) return null;
  const rgb = (Math.trunc(v) >>> 0) & 0xffffff;
  return `#${rgb.toString(16).padStart(6, '0').toUpperCase()}`;
}

export function hexToArgbInt(hex: string): number {
  const clean = hex.replace('#', '').trim();
  const parsed = Number.parseInt(clean.slice(0, 6), 16);
  if (!Number.isFinite(parsed)) return 0xfff5f4f0;
  return 0xff000000 + parsed;
}

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
  dynamicNowPlaying = $state(false);
  performanceMode = $state(false);
  ambientStyle = $state<AmbientStyle>('apple');
  nowPlayingStyle = $state<NowPlayingStyle>('fullBleed');
  nowPlayingAutoHideSec = $state(3);
  miniPlayerStyle = $state<MiniPlayerStyle>('classic');
  closeToTray = $state(true);
  appFont = $state('');
  uiScale = $state(1.0);
  language = $state('system');
  reduceMotion = $state(false);
  private themeSave: Promise<void> | null = null;

  platform = $state<HostPlatform>(detectHostPlatform());
  qualityTier = $state<'auto' | 'high' | 'cdFlac' | 'hiRes96' | 'hiRes192' | 'atmos'>('hiRes192');
  losslessQuality = $state(27);
  downloadQuality = $state(27);
  preferLossless = $state(true);
  loudnessMode = $state<'off' | 'track' | 'album' | 'lufs'>('track');
  exclusiveMode = $state(false);
  bitPerfectEnabled = $state(false);
  gaplessPlayback = $state(true);
  autoplaySimilar = $state(true);
  canvasEnabled = $state(true);
  crossfadeEnabled = $state(false);
  crossfadeSeconds = $state(6);
  autoMixEnabled = $state(false);
  autoMixBars = $state(8);
  autoMixSpectral = $state(true);

  discordRpcEnabled = $state(true);
  smtcEnabled = $state(true);
  scrobbleEnabled = $state(true);
  scrobbleNowPlaying = $state(true);
  scrobblePercent = $state(50);
  primaryScrobbler = $state('');
  lastfmConfigured = $state(false);
  listenBrainzConnected = $state(false);
  listenBrainzUser = $state('');
  listenBrainzServer = $state('');
  libreFmConnected = $state(false);
  libreFmUser = $state('');
  libreFmServer = $state('');

  ytConnected = $state(false);
  ytAccountName = $state('');
  ytAccountEmail = $state('');
  ytAccountAvatar = $state('');
  syncYtHistory = $state(true);
  syncYtLibrary = $state(true);
  podcastsEnabled = $state(true);

  displayName = $state('');
  jamDisplayName = $state('');
  profilePicturePath = $state('');
  profilePictureUrl = $state('');

  spotifyConnected = $state(false);
  spotifyUser = $state('');
  spotifyCanvasEnabled = $state(true);
  preferSpotifyCanvas = $state(false);
  recommendationSource = $state<'youtube' | 'spotify'>('youtube');

  lastfmConnected = $state(false);
  lastfmUser = $state('');

  downloadDir = $state('');
  downloadFolderTemplate = $state('flat');
  embedLyrics = $state(true);
  embedArtwork = $state(true);

  addonsEnabled = $state(true);
  addonUrls = $state<string[]>([]);
  disabledAddonUrls = $state<string[]>([]);

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
    if (mode === 'amoled') {
      this.themeMode = 'amoled';
      this.amoled = true;
    } else if (mode === 'dark') {
      this.themeMode = this.amoled ? 'amoled' : 'dark';
    } else {
      this.themeMode = mode;
    }
    if (customBase) this.customThemeBase = customBase;

    const persistedMode = mode === 'amoled' ? 'dark' : mode;
    const persistedAmoled = mode === 'amoled' ? true : mode === 'dark' ? this.amoled : false;

    this.themeSave = (this.themeSave ?? Promise.resolve()).then(async () => {
      if (customBase) {
        const value = hexToArgbInt(customBase);
        await ipcInvoke('prefs_set_int', { key: 'lw_custom_theme_base', value });
      }
      await ipcInvoke('prefs_set_string', { key: 'lw_theme_mode', value: persistedMode });
      await ipcInvoke('prefs_set_bool', { key: 'lw_amoled', value: persistedAmoled });
    });
  }

  setAmoled(enabled: boolean) {
    this.amoled = enabled;
    if (this.themeMode === 'dark' || this.themeMode === 'amoled') {
      this.themeMode = enabled ? 'amoled' : 'dark';
    }
    this.saveBoolPref('lw_amoled', enabled);
  }

  setAccentColor(hex: string) {
    this.accentColor = hex.toUpperCase();
    this.accentSource = 'custom';
    void ipcInvoke('prefs_set_int', { key: 'lw_accent', value: hexToArgbInt(hex) });
    this.saveStringPref('lw_accent_source', 'custom');
    this.saveStringPref('lw_accent_mode', 'custom');
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
      if (typeof all.amoled === 'boolean') this.amoled = all.amoled;
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
      if (typeof all.dynamicNowPlaying === 'boolean') this.dynamicNowPlaying = all.dynamicNowPlaying;
      if (typeof all.closeToTray === 'boolean') this.closeToTray = all.closeToTray;
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
      if (typeof all.preferLossless === 'boolean') this.preferLossless = all.preferLossless;
      if (typeof all.losslessQuality === 'number') {
        const q = all.losslessQuality;
        this.losslessQuality = q;
        this.qualityTier =
          q >= 28 ? 'atmos' : q >= 27 ? 'hiRes192' : q >= 7 ? 'hiRes96' : q >= 6 ? 'cdFlac' : 'high';
      }
      if (typeof all.downloadQuality === 'number') this.downloadQuality = all.downloadQuality;
      if (typeof all.downloadDir === 'string') this.downloadDir = all.downloadDir;
      if (typeof all.downloadLyrics === 'boolean') this.embedLyrics = all.downloadLyrics;
      if (typeof all.downloadStructure === 'string' && all.downloadStructure) {
        this.downloadFolderTemplate = all.downloadStructure;
      }
      if (typeof all.autoplaySimilar === 'boolean') this.autoplaySimilar = all.autoplaySimilar;
      if (typeof all.canvasEnabled === 'boolean') this.canvasEnabled = all.canvasEnabled;
      if (typeof all.crossfadeEnabled === 'boolean') this.crossfadeEnabled = all.crossfadeEnabled;
      if (typeof all.crossfadeSeconds === 'number') this.crossfadeSeconds = all.crossfadeSeconds;
      if (typeof all.autoMixEnabled === 'boolean') this.autoMixEnabled = all.autoMixEnabled;
      if (typeof all.autoMixBars === 'number') this.autoMixBars = all.autoMixBars === 16 ? 16 : 8;
      if (typeof all.autoMixSpectral === 'boolean') this.autoMixSpectral = all.autoMixSpectral;
      if (typeof all.discordRichPresence === 'boolean') {
        this.discordRpcEnabled = all.discordRichPresence;
      }
      if (typeof all.macosNowPlaying === 'boolean') this.smtcEnabled = all.macosNowPlaying;
      if (typeof all.scrobblerEnabled === 'boolean') this.scrobbleEnabled = all.scrobblerEnabled;
      if (typeof all.scrobbleNowPlaying === 'boolean') this.scrobbleNowPlaying = all.scrobbleNowPlaying;
      if (typeof all.scrobblePercent === 'number') this.scrobblePercent = all.scrobblePercent;
      if (typeof all.primaryScrobbler === 'string') this.primaryScrobbler = all.primaryScrobbler;
      if (typeof all.lastFmConfigured === 'boolean') this.lastfmConfigured = all.lastFmConfigured;
      if (typeof all.listenBrainzConnected === 'boolean') {
        this.listenBrainzConnected = all.listenBrainzConnected;
      }
      if (typeof all.listenBrainzUser === 'string') this.listenBrainzUser = all.listenBrainzUser;
      if (typeof all.listenBrainzServer === 'string') this.listenBrainzServer = all.listenBrainzServer;
      if (typeof all.libreFmConnected === 'boolean') {
        this.libreFmConnected = all.libreFmConnected;
      }
      if (typeof all.libreFmUser === 'string') this.libreFmUser = all.libreFmUser;
      if (typeof all.libreFmServer === 'string') this.libreFmServer = all.libreFmServer;
      if (typeof all.podcastsEnabled === 'boolean') this.podcastsEnabled = all.podcastsEnabled;
      if (typeof all.syncYtLibrary === 'boolean') this.syncYtLibrary = all.syncYtLibrary;
      if (typeof all.syncYtHistory === 'boolean') this.syncYtHistory = all.syncYtHistory;
      if (typeof all.spotifyCanvasEnabled === 'boolean') this.spotifyCanvasEnabled = all.spotifyCanvasEnabled;
      if (typeof all.preferSpotifyCanvas === 'boolean') this.preferSpotifyCanvas = all.preferSpotifyCanvas;
      if (all.recommendationSource === 'spotify' || all.recommendationSource === 'youtube') {
        this.recommendationSource = all.recommendationSource;
      }
      if (all.hazeMaterial === 'automatic' || all.hazeMaterial === 'haze' || all.hazeMaterial === 'solid') {
        this.hazeMaterial = all.hazeMaterial;
      }
      if (all.hazeIntensity === 'low' || all.hazeIntensity === 'medium' || all.hazeIntensity === 'high') {
        this.hazeIntensity = all.hazeIntensity;
      }
      if (
        all.ambientStyle === 'apple' ||
        all.ambientStyle === 'soft' ||
        all.ambientStyle === 'aurora' ||
        all.ambientStyle === 'wash' ||
        all.ambientStyle === 'mesh' ||
        all.ambientStyle === 'off'
      ) {
        this.ambientStyle = all.ambientStyle;
      }
      if (typeof all.addonsEnabled === 'boolean') this.addonsEnabled = all.addonsEnabled;
      if (Array.isArray(all.addonUrls)) {
        this.addonUrls = all.addonUrls.filter((u): u is string => typeof u === 'string');
      }
      if (Array.isArray(all.disabledAddonUrls)) {
        this.disabledAddonUrls = all.disabledAddonUrls.filter((u): u is string => typeof u === 'string');
      }
    }
    if (lastfm) {
      this.lastfmConnected = Boolean(lastfm.username) && lastfm.status === 'signedIn';
      this.lastfmUser = lastfm.username || '';
    }
    const reduceMot = await ipcInvoke<boolean | null>('prefs_get', { key: 'lw_reduce_motion' }).catch(
      () => null
    );
    if (typeof reduceMot === 'boolean') this.reduceMotion = reduceMot;
    // Performance mode intentionally stays disabled in Tauri build
    this.performanceMode = false;
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
