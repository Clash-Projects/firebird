import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { settingsStore } from './settings.svelte';
import type {
  AudioDevice,
  LyricsDocument,
  PodcastShelfItem,
  PodcastShow,
  ToastMessage,
  Track,
} from './types';

export const LYRICS_PROVIDERS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'auto', label: 'Auto' },
  { id: 'lastwave', label: 'LastWave' },
  { id: 'lrc_red', label: 'lrc.red' },
  { id: 'apple_music', label: 'Apple Music' },
  { id: 'better_lyrics', label: 'BetterLyrics' },
  { id: 'better_lyrics_portato', label: 'BetterLyrics (Portato)' },
  { id: 'kugou', label: 'KuGou' },
  { id: 'bini_lyrics', label: 'BiniLyrics' },
  { id: 'musixmatch', label: 'Musixmatch' },
  { id: 'lrclib', label: 'LRCLIB' },
  { id: 'lyrics_plus', label: 'LyricsPlus' },
  { id: 'unison', label: 'Unison' },
  { id: 'youtube_captions', label: 'YouTube Captions' },
  { id: 'youtube_music', label: 'YouTube Music' },
  { id: 'megalobiz', label: 'Megalobiz' },
  { id: 'genius', label: 'Genius' },
];

export const LYRICS_ANIMATION_OPTIONS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'apple_fluid', label: 'Apple Fluid' },
  { id: 'karaoke_bounce', label: 'Rhythm Bounce' },
  { id: 'kinetic_pop', label: 'Kinetic Pop' },
  { id: 'neon_cyber', label: 'Neon Cyber' },
  { id: 'italic_wave', label: 'Silk Italic' },
  { id: 'glitch_snap', label: 'Chromatic Glitch' },
  { id: 'elastic_tilt', label: '3D Stage Tilt' },
  { id: 'hyper_warp', label: 'Hyperdrive Warp' },
  { id: 'chaos_vortex', label: 'Chaos Vortex' },
];

export const LYRICS_TEXT_SCALES: ReadonlyArray<{ scale: number; label: string }> = [
  { scale: 0.85, label: 'Small' },
  { scale: 1.0, label: 'Medium' },
  { scale: 1.2, label: 'Large' },
  { scale: 1.4, label: 'Extra large' },
  { scale: 1.6, label: 'Huge' },
];

export const LYRICS_TRANSLATION_OPTIONS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'off', label: 'Off' },
  { id: 'en', label: 'English' },
  { id: 'es', label: 'Spanish' },
  { id: 'fr', label: 'French' },
  { id: 'de', label: 'German' },
  { id: 'pt', label: 'Portuguese' },
  { id: 'hi', label: 'Hindi' },
  { id: 'ja', label: 'Japanese' },
  { id: 'ko', label: 'Korean' },
];

export const EQ_FREQUENCIES = [
  31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000,
];

export interface ClarityPreset {
  key: 'reference' | 'speaker' | 'headphone' | 'dac';
  displayName: string;
  description: string;
  trimsDb: number[];
}

export const CLARITY_PRESETS: ReadonlyArray<ClarityPreset> = [
  {
    key: 'reference',
    displayName: 'Reference',
    description: 'Unmodified audiophile reference curve; all stage trims 0 dB.',
    trimsDb: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
  },
  {
    key: 'speaker',
    displayName: 'Speaker',
    description: 'Reduced sub-bass (-1.5 dB) and gentler air (-0.5 dB) for small desktop drivers.',
    trimsDb: [0.0, -1.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0],
  },
  {
    key: 'headphone',
    displayName: 'Headphone',
    description: 'Lighter bass (-0.5 dB) and softer presence (-1.0 dB) for close-coupled drivers.',
    trimsDb: [0.0, -0.5, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0],
  },
  {
    key: 'dac',
    displayName: 'DAC',
    description: 'Gentler air (-1.5 dB shelf, -1.0 dB exciter) for revealing external DAC chains.',
    trimsDb: [0.0, 0.0, 0.0, 0.0, 0.0, -1.5, 0.0, -1.0],
  },
];

export const EQ_PRESETS: Record<string, number[]> = {
  Default: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
  'Studio Master': [2.7, 2.2, 0.6, -2.6, -0.6, 1.2, 3.0, 4.0, 4.3, 4.8],
  'Vocal Clarity': [-1.2, -0.5, 0.0, 1.0, 2.5, 3.5, 2.8, 2.0, 1.5, 1.0],
  'Deep Bass Clean': [4.8, 3.5, 2.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5],
  'Bass Boost': [6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
  Warm: [2.0, 2.0, 1.5, 1.0, 0.5, 0.0, -0.5, -1.2, -2.0, -2.8],
  Bright: [-1.0, -0.5, 0.0, 0.0, 0.0, 0.5, 1.5, 3.0, 4.0, 4.5],
  'Late Night': [1.5, 1.0, 0.5, 0.0, 0.0, 0.5, 1.0, 1.0, 0.5, 0.0],
  Rock: [3.8, 3.0, 1.5, -1.0, -0.8, 0.5, 2.0, 3.0, 3.8, 4.0],
  Pop: [-0.8, 0.0, 1.0, 2.0, 1.8, 1.0, 0.5, 1.0, 1.8, 2.5],
  Electronic: [4.2, 3.0, 2.0, -0.5, 0.5, 2.0, 2.5, 2.5, 3.2, 4.0],
  'R&B': [4.8, 3.5, 2.0, -0.5, -0.8, 0.5, 1.8, 2.5, 3.0, 3.0],
  Acoustic: [2.8, 2.0, 1.5, 1.0, 1.8, 2.0, 1.2, 1.0, 1.2, 1.5],
  Classical: [2.8, 2.0, 1.5, 0.0, 0.0, 0.5, 1.8, 2.0, 2.8, 3.0],
  Jazz: [2.2, 1.5, 1.0, -0.5, 0.2, 1.0, 1.8, 2.0, 2.5, 2.5],
  'Treble Air': [0.0, 0.0, -0.5, -0.5, 0.2, 1.0, 1.8, 3.0, 4.0, 5.0],
  'Speech / Podcast': [-4.0, -3.0, -1.5, 0.0, 1.5, 3.0, 3.5, 2.5, 0.5, -1.0],
};

export interface JamChatMessage {
  id: string;
  sender: string;
  text: string;
  time: string;
}

interface BackendDacDevice {
  id: string;
  name: string;
  enumerator?: string;
  exclusiveSupported?: boolean;
  isDefault?: boolean;
  formats?: Array<{ sampleRate?: number; bitDepth?: number }>;
}

interface BackendAudioOutputState {
  platform?: 'windows' | 'macos' | 'linux';
  devices?: BackendDacDevice[];
  selectedId?: string;
  exclusiveRequested?: boolean;
  bitPerfectRequested?: boolean;
}

interface BackendLyricsResult {
  lines?: Array<{
    timeMs?: number;
    durationMs?: number;
    text?: string;
    transliteration?: string;
    syllables?: Array<{ timeMs?: number; durationMs?: number; text?: string }>;
  }>;
  plainLyrics?: string;
  isSynced?: boolean;
  isWordSynced?: boolean;
  isInstrumental?: boolean;
  source?: string;
}

interface BackendPodcastShelf {
  title?: string;
  shows?: Array<{
    showId: string;
    title: string;
    publisher?: string;
    description?: string;
    artworkUrl?: string;
  }>;
  episodes?: Array<{
    videoId: string;
    title: string;
    showId?: string;
    showTitle?: string;
    description?: string;
    publishedLabel?: string;
    durationMs?: number;
    artworkUrl?: string;
  }>;
}

class FeaturesStore {
  eqEnabled = $state(false);
  eqPreset = $state('Default');
  eqBands = $state<number[]>([...EQ_PRESETS.Default]);
  eqPreampDb = $state(0);
  studioClarity = $state(false);
  clarityPresetKey = $state<'reference' | 'speaker' | 'headphone' | 'dac'>('reference');
  headphoneProfile = $state('off');

  devices = $state<AudioDevice[]>([]);
  selectedDeviceId = $state('');

  lyricsByTrack = $state<Record<string, LyricsDocument>>({});
  lyricsProviderId = $state('auto');
  wordByWord = $state(true);
  lyricsLoading = $state(false);
  lyricsOffsetMs = $state<Record<string, number>>({});
  showTranslation = $state(false);
  lyricsTranslationLang = $state('off');
  showRomanized = $state(false);
  lyricsFontScale = $state(1.0);
  lyricsAnimation = $state('apple_fluid');

  podcasts = $state<PodcastShow[]>([]);
  podcastShelves = $state<PodcastShelfItem[]>([]);
  podcastsLoading = $state(false);
  podcastsError = $state<string | null>(null);

  jamConnected = $state(false);
  jamRoomCode = $state('');
  jamIsHost = $state(false);
  jamParticipants = $state<{ id: string; name: string; role: string }[]>([]);
  jamChat = $state<JamChatMessage[]>([]);

  toasts = $state<ToastMessage[]>([]);

  private lyricsInFlight = new Set<string>();

  selectedDevice = $derived<AudioDevice | undefined>(
    this.devices.find((d) => d.id === this.selectedDeviceId) ?? this.devices[0]
  );

  async connectBackend() {
    if (!isTauriRuntime()) return;
    await Promise.allSettled([
      this.refreshAudioOutput(),
      this.refreshEqState(),
      this.loadPodcasts(),
    ]);
  }

  async refreshEqState() {
    if (!isTauriRuntime()) return;
    const eq = await ipcInvoke<{
      enabled?: boolean;
      presetName?: string;
      gainsDb?: number[];
      studioClarityEnabled?: boolean;
      clarityPresetKey?: string;
    }>('eq_get_state');
    if (!eq) return;
    if (typeof eq.enabled === 'boolean') this.eqEnabled = eq.enabled;
    if (typeof eq.studioClarityEnabled === 'boolean') this.studioClarity = eq.studioClarityEnabled;
    if (typeof eq.clarityPresetKey === 'string') {
      const matched = CLARITY_PRESETS.find((p) => p.key === eq.clarityPresetKey);
      if (matched) this.clarityPresetKey = matched.key;
    }
    if (typeof eq.presetName === 'string' && eq.presetName) this.eqPreset = eq.presetName;
    if (Array.isArray(eq.gainsDb) && eq.gainsDb.length === this.eqBands.length) {
      this.eqBands = [...eq.gainsDb];
    }
  }

  hydrateFromPrefs(all: Record<string, unknown> | null) {
    if (!all) return;
    if (typeof all.lyricsProviderId === 'string' && all.lyricsProviderId) {
      this.lyricsProviderId = all.lyricsProviderId;
    }
    if (typeof all.wordByWord === 'boolean') this.wordByWord = all.wordByWord;
    if (typeof all.lyricsTransliteration === 'boolean') {
      this.showRomanized = all.lyricsTransliteration;
    }
    if (typeof all.lyricsTranslationLang === 'string' && all.lyricsTranslationLang) {
      this.lyricsTranslationLang = all.lyricsTranslationLang;
      this.showTranslation = all.lyricsTranslationLang !== 'off';
    }
    if (typeof all.lyricsTextScale === 'number' && all.lyricsTextScale > 0) {
      this.lyricsFontScale = all.lyricsTextScale;
    }
    if (typeof all.lyricsAnimation === 'string' && all.lyricsAnimation) {
      this.lyricsAnimation = all.lyricsAnimation;
    }
    if (typeof all.headphoneProfile === 'string' && all.headphoneProfile) {
      this.headphoneProfile = all.headphoneProfile;
    }
  }

  async refreshAudioOutput() {
    if (!isTauriRuntime()) return;
    const state = await ipcInvoke<BackendAudioOutputState>('audio_output_state');
    if (!state) return;

    if (state.platform === 'windows' || state.platform === 'macos' || state.platform === 'linux') {
      settingsStore.platform = state.platform;
    }
    if (typeof state.exclusiveRequested === 'boolean') {
      settingsStore.exclusiveMode = state.exclusiveRequested;
    }
    if (typeof state.bitPerfectRequested === 'boolean') {
      settingsStore.bitPerfectEnabled = state.bitPerfectRequested;
    }
    if (Array.isArray(state.devices)) {
      const driver =
        state.platform === 'macos'
          ? 'CoreAudio'
          : state.platform === 'linux'
            ? 'PipeWire'
            : 'WASAPI';
      this.devices = state.devices.map((d) => {
        const maxSampleRateHz = (d.formats ?? []).reduce(
          (max, f) => Math.max(max, Number(f.sampleRate) || 0),
          48000
        );
        const maxBitDepth = (d.formats ?? []).reduce(
          (max, f) => Math.max(max, Number(f.bitDepth) || 0),
          24
        );
        return {
          id: d.id,
          name: d.name,
          driver,
          supportsExclusive: Boolean(d.exclusiveSupported),
          maxSampleRateHz,
          maxBitDepth,
          isDefault: Boolean(d.isDefault),
        };
      });
    }
    if (typeof state.selectedId === 'string' && state.selectedId) {
      this.selectedDeviceId = state.selectedId;
    }
  }

  selectDevice(deviceId: string) {
    this.selectedDeviceId = deviceId;
    void ipcInvoke('audio_output_select_device', { id: deviceId });
  }

  async loadPodcasts(force = false) {
    if (!isTauriRuntime()) return;
    this.podcastsLoading = true;
    this.podcastsError = null;
    try {
      const shelves = await ipcInvoke<BackendPodcastShelf[]>('podcasts_home', { force });
      if (!Array.isArray(shelves)) return;

      const byId = new Map<string, PodcastShow>();
      const mappedShelves: PodcastShelfItem[] = [];

      for (const shelf of shelves) {
        const shows: PodcastShow[] = (shelf.shows ?? [])
          .filter((s) => Boolean(s.showId))
          .map((s) => ({
            id: s.showId,
            title: s.title,
            publisher: s.publisher || '',
            description: s.description || '',
            artworkUrl: s.artworkUrl || '',
            episodes: [],
          }));

        for (const s of shows) {
          if (!byId.has(s.id)) byId.set(s.id, s);
        }

        const episodes = (shelf.episodes ?? [])
          .filter((ep) => Boolean(ep.videoId))
          .map((ep) => ({
            id: ep.videoId,
            showId: ep.showId || '',
            showTitle: ep.showTitle || '',
            title: ep.title,
            description: ep.description || ep.showTitle || '',
            publishedAt: ep.publishedLabel || '',
            durationSec: Math.max(0, Math.round(Number(ep.durationMs || 0) / 1000)),
            resumeSec: 0,
            artworkUrl: ep.artworkUrl || '',
          }));

        if (shows.length > 0 || episodes.length > 0) {
          mappedShelves.push({
            title: shelf.title || 'Podcasts',
            shows,
            episodes,
          });
        }
      }

      this.podcastShelves = mappedShelves;
      this.podcasts = Array.from(byId.values());
    } catch (err) {
      this.podcastsError = err instanceof Error ? err.message : 'Failed to load podcasts';
    } finally {
      this.podcastsLoading = false;
    }
  }

  getLyricsOffset(trackId: string): number {
    return this.lyricsOffsetMs[trackId] ?? 0;
  }

  adjustLyricsOffset(trackId: string, deltaMs: number) {
    const next = (this.lyricsOffsetMs[trackId] ?? 0) + deltaMs;
    this.lyricsOffsetMs = { ...this.lyricsOffsetMs, [trackId]: next };
  }

  resetLyricsOffset(trackId: string) {
    this.lyricsOffsetMs = { ...this.lyricsOffsetMs, [trackId]: 0 };
  }

  async fetchLyricsForTrack(
    track: Track,
    opts?: { forceRefresh?: boolean; providerId?: string }
  ) {
    const force = Boolean(opts?.forceRefresh);
    const providerId = opts?.providerId ?? this.lyricsProviderId ?? 'auto';
    if (!isTauriRuntime() || !track.id) return;
    if (!force && (this.lyricsByTrack[track.id] || this.lyricsInFlight.has(track.id))) {
      return;
    }
    this.lyricsInFlight.add(track.id);
    this.lyricsLoading = true;
    try {
      const res = await ipcInvoke<BackendLyricsResult>('lyrics_fetch', {
        title: track.title,
        artist: track.artist,
        album: track.album,
        durationSeconds: track.durationSec > 0 ? Math.round(track.durationSec) : null,
        forceRefresh: force,
        wordByWord: this.wordByWord,
        videoId: track.videoId || null,
        preferredProviderId: providerId,
        excludeProviderIds: [],
      });
      if (!res) return;

      const rawLines = Array.isArray(res.lines) ? res.lines : [];
      let mappedLines = rawLines.map((l) => {
        const startMs = Math.max(0, Number(l.timeMs) || 0);
        const dur = Math.max(0, Number(l.durationMs) || 0);
        return {
          startMs,
          endMs: startMs + dur,
          text: l.text || '',
          romanized: l.transliteration || undefined,
          words:
            l.syllables && l.syllables.length > 0
              ? l.syllables.map((s) => {
                  const ws = Math.max(0, Number(s.timeMs) || startMs);
                  const wd = Math.max(0, Number(s.durationMs) || 0);
                  return { text: s.text || '', startMs: ws, endMs: ws + Math.max(80, wd) };
                })
              : undefined,
        };
      });

      if (mappedLines.length === 0 && res.plainLyrics?.trim()) {
        mappedLines = res.plainLyrics
          .split(/\r?\n/)
          .map((t) => t.trim())
          .filter(Boolean)
          .map((text, idx) => ({
            startMs: idx * 1000,
            endMs: (idx + 1) * 1000,
            text,
            romanized: undefined,
            words: undefined,
          }));
      }

      if (mappedLines.length === 0) return;

      this.lyricsByTrack = {
        ...this.lyricsByTrack,
        [track.id]: {
          trackId: track.id,
          provider: res.source || providerId,
          synced: Boolean(res.isSynced && rawLines.length > 0),
          wordSynced: Boolean(res.isWordSynced),
          lines: mappedLines,
        },
      };
    } finally {
      this.lyricsInFlight.delete(track.id);
      this.lyricsLoading = false;
    }
  }

  get combinedEqGainsDb(): number[] {
    const clarity = CLARITY_PRESETS.find((p) => p.key === this.clarityPresetKey) ?? CLARITY_PRESETS[0];
    const sm = EQ_PRESETS['Studio Master'];
    return EQ_FREQUENCIES.map((_, i) => {
      let gain = this.eqEnabled ? (this.eqBands[i] ?? 0) : 0;
      if (this.studioClarity) {
        let trim = 0;
        if (i === 1) trim = clarity.trimsDb[1];
        else if (i === 3) trim = clarity.trimsDb[2];
        else if (i === 4) trim = clarity.trimsDb[3];
        else if (i === 6 || i === 7) trim = clarity.trimsDb[4];
        else if (i >= 8) {
          trim = clarity.trimsDb[5];
          if (i === 9) trim += clarity.trimsDb[7];
        }
        gain += (sm[i] ?? 0) + trim;
      }
      return Math.max(-8, Math.min(8, gain));
    });
  }

  setEqEnabled(enabled: boolean) {
    this.eqEnabled = enabled;
    void ipcInvoke('eq_set_enabled', { enabled });
  }

  setStudioClarityEnabled(enabled: boolean) {
    this.studioClarity = enabled;
    void ipcInvoke('eq_set_studio_clarity_enabled', { enabled });
  }

  setClarityPreset(key: ClarityPreset['key']) {
    this.clarityPresetKey = key;
    void ipcInvoke('eq_set_clarity_preset', { key });
  }

  resetEq() {
    this.eqPreset = 'Default';
    this.eqBands = [...EQ_PRESETS.Default];
    void ipcInvoke('eq_reset');
  }

  applyEqPreset(name: string) {
    const preset = EQ_PRESETS[name];
    if (!preset) return;
    this.eqPreset = name;
    this.eqBands = [...preset];
    void ipcInvoke('eq_set_preset', { presetName: name });
  }

  setEqBand(index: number, db: number) {
    const clamped = Math.max(-8, Math.min(8, Math.round(db * 10) / 10));
    this.eqBands[index] = clamped;
    this.eqPreset = 'Custom';
    void ipcInvoke('eq_set_band_gain', { bandIndex: index, gainDb: clamped });
  }

  getLyrics(trackId: string): LyricsDocument | null {
    return this.lyricsByTrack[trackId] ?? null;
  }

  setLyricsRomanized(enabled: boolean) {
    this.showRomanized = enabled;
    settingsStore.saveBoolPref('lw_lyrics_transliteration', enabled);
  }

  setLyricsTranslationLang(lang: string) {
    this.lyricsTranslationLang = lang;
    this.showTranslation = lang !== 'off';
    settingsStore.saveStringPref('lw_lyrics_translation', lang);
  }

  setLyricsTextScale(scale: number) {
    this.lyricsFontScale = scale;
    settingsStore.saveDoublePref('lw_lyrics_text_scale', scale);
  }

  setLyricsAnimation(animation: string) {
    this.lyricsAnimation = animation;
    settingsStore.saveStringPref('lw_lyrics_animation', animation);
  }

  sendJamMessage(text: string) {
    const clean = text.trim();
    if (!clean) return;
    const now = new Date();
    const time = `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}`;
    this.jamChat = [...this.jamChat, { id: `m-${Date.now()}`, sender: 'You', text: clean, time }];
    void ipcInvoke('jam_send_chat', { text: clean, replyToId: null });
  }

  showToast(toast: Omit<ToastMessage, 'id'>, durationMs = 3600) {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    this.toasts = [...this.toasts, { ...toast, id }];
    setTimeout(() => {
      this.dismissToast(id);
    }, durationMs);
  }

  dismissToast(id: string) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const featuresStore = new FeaturesStore();
