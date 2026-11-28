import { ipcInvoke, ipcListen, isTauriRuntime } from '$src/lib/ipc';
import { needsRomanization } from '$src/features/lyrics/lyricAdapter';
import { playerStore } from './player.svelte';
import { settingsStore } from './settings.svelte';
import type {
  AudioDevice,
  LyricLine,
  LyricsDocument,
  PodcastShelfItem,
  PodcastShow,
  ToastMessage,
  Track,
} from './types';

export const LYRICS_PROVIDERS: ReadonlyArray<{ id: string; label: string; subtitle: string }> = [
  { id: 'auto', label: 'Auto', subtitle: 'Fastest word-sync wins, LRCLIB fallback' },
  { id: 'lastwave', label: 'LastWave', subtitle: 'Syllable & line-synced Apple lyrics first' },
  { id: 'lrc_red', label: 'lrc.red', subtitle: 'ISRC-indexed syllable TTML & LRC first' },
  { id: 'apple_music', label: 'Apple Music', subtitle: 'Syllable-synced Apple Music lyrics first' },
  { id: 'better_lyrics', label: 'BetterLyrics', subtitle: 'Word-synced lyrics first' },
  { id: 'better_lyrics_portato', label: 'QQ Karaoke', subtitle: 'QQ Music karaoke word timings first' },
  { id: 'kugou', label: 'Kugou', subtitle: 'KRC word-synced lyrics first' },
  { id: 'bini_lyrics', label: 'Syllable-Sync', subtitle: 'Recording-matched syllable lyrics first' },
  { id: 'musixmatch', label: 'Catalog', subtitle: 'Largest catalogue line-sync first' },
  { id: 'lrclib', label: 'LRCLIB', subtitle: 'Line-synced community lyrics first' },
  { id: 'lyrics_plus', label: 'LyricsPlus', subtitle: 'Syllable-synced on community mirrors first' },
  { id: 'unison', label: 'Unison', subtitle: 'Community-submitted synced lyrics first' },
  { id: 'youtube_captions', label: 'YouTube Captions', subtitle: 'Timed captions from the playing video first' },
  { id: 'youtube_music', label: 'YouTube Music', subtitle: "Plain lyrics from the video's Lyrics tab first" },
  { id: 'megalobiz', label: 'Megalobiz', subtitle: 'Community line-synced LRC first' },
  { id: 'genius', label: 'Genius', subtitle: 'Plain-text web catalogue, tried last' },
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

export const LYRICS_AUTO_HIDE_OPTIONS: ReadonlyArray<{ seconds: number; label: string }> = [
  { seconds: 0, label: 'Always on' },
  { seconds: 2, label: '2 seconds' },
  { seconds: 4, label: '4 seconds' },
  { seconds: 6, label: '6 seconds' },
  { seconds: 8, label: '8 seconds' },
  { seconds: 10, label: '10 seconds' },
];

export function lyricsSourceToProviderId(source: string): string | null {
  const s = source.trim().toLowerCase();
  if (!s) return null;
  if (s.startsWith('lastwave')) return 'lastwave';
  if (s.startsWith('lrc.red')) return 'lrc_red';
  if (s.startsWith('apple music') || s.includes('apple')) return 'apple_music';
  if (s.startsWith('qq karaoke') || s.startsWith('portato')) return 'better_lyrics_portato';
  if (s.startsWith('betterlyrics')) return 'better_lyrics';
  if (s.startsWith('kugou')) return 'kugou';
  if (s.startsWith('syllable-sync') || s.startsWith('bini')) return 'bini_lyrics';
  if (s.startsWith('catalog') || s.startsWith('musixmatch')) return 'musixmatch';
  if (s.includes('lrclib')) return 'lrclib';
  if (s.startsWith('lyricsplus')) return 'lyrics_plus';
  if (s.startsWith('unison')) return 'unison';
  if (s.startsWith('youtube captions')) return 'youtube_captions';
  if (s.startsWith('youtube music')) return 'youtube_music';
  if (s.startsWith('megalobiz')) return 'megalobiz';
  if (s.startsWith('genius')) return 'genius';
  return null;
}

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
    agent?: string;
    syllables?: Array<{
      timeMs?: number;
      durationMs?: number;
      text?: string;
      isBackground?: boolean;
    }>;
  }>;
  plainLyrics?: string;
  isSynced?: boolean;
  isWordSynced?: boolean;
  isInstrumental?: boolean;
  source?: string;
}

interface BackendLyricsPartialEvent {
  trackId: string;
  requestId: string;
  result: BackendLyricsResult;
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
  lyricsErrorByTrack = $state<Record<string, string>>({});
  lyricsProviderId = $state('auto');
  lyricsProviderOverrides = $state<Record<string, string | null>>({});
  lyricsExcludedProviders = $state<Record<string, string[]>>({});
  lyricsFetchedDurationSec = $state<Record<string, number>>({});
  wordByWord = $state(true);
  lyricsLoading = $state(false);
  lyricsOffsetMs = $state<Record<string, number>>({});
  showTranslation = $state(false);
  lyricsTranslationLang = $state('off');
  showRomanized = $state(true);
  lyricsFontScale = $state(1.0);
  lyricsAnimation = $state('apple_fluid');
  lyricsAutoHideSeconds = $state(0);

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
  private lyricsRequestIds: Record<string, string> = {};
  private loadedOffsets = new Set<string>();
  private translationMemory = new Map<string, Array<string | null>>();
  private romanizeMemory = new Map<string, Array<string | null>>();
  private unlistenPartial: (() => void) | null = null;

  selectedDevice = $derived<AudioDevice | undefined>(
    this.devices.find((d) => d.id === this.selectedDeviceId) ?? this.devices[0]
  );

  async connectBackend() {
    if (!isTauriRuntime()) return;
    if (!this.unlistenPartial) {
      void ipcListen<BackendLyricsPartialEvent>('lyrics-partial', (ev) => {
        if (!ev?.trackId || !ev.result) return;
        if (ev.requestId && this.lyricsRequestIds[ev.trackId] !== ev.requestId) return;
        const effectiveProv = this.getEffectiveProviderId(ev.trackId);
        this.applyBackendLyricsResult(ev.trackId, ev.result, effectiveProv);
      }).then((fn) => {
        this.unlistenPartial = fn ?? null;
      });
    }
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
    if (typeof all.lyricsAutoHideSeconds === 'number' && all.lyricsAutoHideSeconds >= 0) {
      this.lyricsAutoHideSeconds = all.lyricsAutoHideSeconds;
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

  async loadLyricsOffset(trackId: string) {
    if (!isTauriRuntime() || !trackId || this.loadedOffsets.has(trackId)) return;
    this.loadedOffsets.add(trackId);
    const saved = await ipcInvoke<number>('lyrics_get_offset', { trackKey: trackId });
    if (typeof saved === 'number' && Number.isFinite(saved)) {
      this.lyricsOffsetMs = { ...this.lyricsOffsetMs, [trackId]: saved };
    }
  }

  getLyricsOffset(trackId: string): number {
    return this.lyricsOffsetMs[trackId] ?? 0;
  }

  adjustLyricsOffset(trackId: string, deltaMs: number) {
    const next = (this.lyricsOffsetMs[trackId] ?? 0) + deltaMs;
    this.lyricsOffsetMs = { ...this.lyricsOffsetMs, [trackId]: next };
    if (isTauriRuntime()) {
      void ipcInvoke('lyrics_set_offset', { trackKey: trackId, offsetMs: next });
    }
  }

  resetLyricsOffset(trackId: string) {
    this.lyricsOffsetMs = { ...this.lyricsOffsetMs, [trackId]: 0 };
    if (isTauriRuntime()) {
      void ipcInvoke('lyrics_reset_offset', { trackKey: trackId });
    }
  }

  getEffectiveProviderId(trackId?: string | null): string {
    if (trackId && this.lyricsProviderOverrides[trackId]) {
      return this.lyricsProviderOverrides[trackId]!;
    }
    return this.lyricsProviderId || 'auto';
  }

  /**
   * Per-track provider selection matching OG Flutter `_SourceMenu` & `NowPlayingOverflowMenu`:
   * - `pickedId === null`: "Try another source" — keeps override, adds current winner to excludes.
   * - `pickedId === 'auto'`: resets per-track override & excludes to Auto.
   * - `pickedId === '<provider>'`: pins per-track override AND excludes previous winner from fallback.
   */
  selectTrackLyricsProvider(track: Track, pickedId: string | null) {
    const currentSource = this.lyricsByTrack[track.id]?.provider ?? '';
    const currentId = lyricsSourceToProviderId(currentSource);
    const prevExcludes = this.lyricsExcludedProviders[track.id] ?? [];

    if (pickedId === null) {
      const nextExcludes =
        currentId && !prevExcludes.includes(currentId)
          ? [...prevExcludes, currentId]
          : [...prevExcludes];
      this.lyricsExcludedProviders = {
        ...this.lyricsExcludedProviders,
        [track.id]: nextExcludes,
      };
    } else if (pickedId === 'auto') {
      this.lyricsProviderOverrides = {
        ...this.lyricsProviderOverrides,
        [track.id]: 'auto',
      };
      this.lyricsExcludedProviders = {
        ...this.lyricsExcludedProviders,
        [track.id]: [],
      };
    } else {
      const nextExcludes =
        currentId && currentId !== pickedId && !prevExcludes.includes(currentId)
          ? [...prevExcludes, currentId]
          : [...prevExcludes];
      this.lyricsProviderOverrides = {
        ...this.lyricsProviderOverrides,
        [track.id]: pickedId,
      };
      this.lyricsExcludedProviders = {
        ...this.lyricsExcludedProviders,
        [track.id]: nextExcludes,
      };
    }

    void this.fetchLyricsForTrack(track, { forceRefresh: true });
  }

  setDefaultLyricsProvider(providerId: string) {
    this.lyricsProviderId = providerId;
    settingsStore.saveStringPref('lw_lyrics_provider', providerId);
    const cur = playerStore.currentTrack;
    if (cur) {
      // Clear per-track override so changing default in Settings updates Now Playing immediately
      const nextOverrides = { ...this.lyricsProviderOverrides };
      delete nextOverrides[cur.id];
      this.lyricsProviderOverrides = nextOverrides;
      this.lyricsExcludedProviders = { ...this.lyricsExcludedProviders, [cur.id]: [] };
      void this.fetchLyricsForTrack(cur, { forceRefresh: true, providerId });
    }
  }

  private applyBackendLyricsResult(
    trackId: string,
    res: BackendLyricsResult,
    fallbackProvider: string
  ) {
    if (res.isInstrumental) {
      this.lyricsByTrack = {
        ...this.lyricsByTrack,
        [trackId]: {
          trackId,
          provider: res.source || fallbackProvider,
          synced: false,
          wordSynced: false,
          isInstrumental: true,
          lines: [],
        },
      };
      return;
    }

    const rawLines = Array.isArray(res.lines) ? res.lines : [];
    let mappedLines: LyricLine[] = rawLines.map((l) => {
      const startMs = Math.max(0, Number(l.timeMs) || 0);
      const dur = Math.max(0, Number(l.durationMs) || 0);
      return {
        startMs,
        endMs: startMs + dur,
        text: l.text || '',
        romanized: l.transliteration || undefined,
        agent: l.agent || undefined,
        words:
          l.syllables && l.syllables.length > 0
            ? l.syllables.map((s) => {
                const ws = Math.max(0, Number(s.timeMs) || startMs);
                const wd = Math.max(0, Number(s.durationMs) || 0);
                return {
                  text: s.text || '',
                  startMs: ws,
                  endMs: ws + Math.max(80, wd),
                  isBackground: Boolean(s.isBackground),
                };
              })
            : undefined,
      };
    });

    if (mappedLines.length === 0 && res.plainLyrics?.trim()) {
      mappedLines = res.plainLyrics
        .split(/\r?\n/)
        .map((t) => t.trim())
        .filter(Boolean)
        .map((text) => ({
          startMs: 0,
          endMs: 0,
          text,
          romanized: undefined,
          words: undefined,
        }));
    }

    // Preserve previously resolved translations/romanisations if line texts match
    const prevDoc = this.lyricsByTrack[trackId];
    if (prevDoc && prevDoc.lines.length === mappedLines.length) {
      mappedLines = mappedLines.map((line, i) => {
        const prev = prevDoc.lines[i];
        if (prev && prev.text === line.text) {
          return {
            ...line,
            translation: line.translation ?? prev.translation,
            romanized: line.romanized ?? prev.romanized,
          };
        }
        return line;
      });
    }

    this.lyricsByTrack = {
      ...this.lyricsByTrack,
      [trackId]: {
        trackId,
        provider: res.source || fallbackProvider,
        synced: Boolean(res.isSynced && rawLines.length > 0),
        wordSynced: Boolean(res.isWordSynced),
        isInstrumental: false,
        lines: mappedLines,
      },
    };

    if (mappedLines.length > 0) {
      void this.enrichLyricsForTrack(trackId);
    }
  }

  async enrichLyricsForTrack(trackId: string) {
    if (!isTauriRuntime()) return;
    const doc = this.lyricsByTrack[trackId];
    if (!doc || doc.lines.length === 0) return;

    const lineTexts = doc.lines.map((l) => l.text);
    const fp = lineTexts.join('\n');

    // 1. Live Translation
    const targetLang = this.showTranslation ? this.lyricsTranslationLang : 'off';
    if (targetLang && targetLang !== 'off') {
      const cacheKey = `${trackId}|${targetLang}|${fp}`;
      let translated = this.translationMemory.get(cacheKey);
      if (!translated) {
        const res = await ipcInvoke<Array<string | null>>('lyrics_translate', {
          trackKey: trackId,
          target: targetLang,
          lines: lineTexts,
        }).catch(() => null);
        if (Array.isArray(res) && res.length === lineTexts.length) {
          translated = res;
          this.translationMemory.set(cacheKey, res);
        }
      }
      const latest = this.lyricsByTrack[trackId];
      if (translated && latest && latest.lines.length === translated.length) {
        const nextLines = latest.lines.map((l, idx) => ({
          ...l,
          translation: translated![idx] ?? undefined,
        }));
        this.lyricsByTrack = {
          ...this.lyricsByTrack,
          [trackId]: { ...latest, lines: nextLines },
        };
      }
    }

    // 2. Phonetic Romanisation fallback for non-Latin scripts lacking provider transliteration
    if (this.showRomanized) {
      const currentDoc = this.lyricsByTrack[trackId];
      if (!currentDoc) return;
      const missingRoman = currentDoc.lines.some(
        (l) => !l.romanized && needsRomanization(l.text)
      );
      if (missingRoman) {
        const romKey = `rom|${trackId}|${fp}`;
        let romanized = this.romanizeMemory.get(romKey);
        if (!romanized) {
          const res = await ipcInvoke<Array<string | null>>('lyrics_romanize', {
            trackKey: trackId,
            lines: lineTexts,
          }).catch(() => null);
          if (Array.isArray(res) && res.length === lineTexts.length) {
            romanized = res;
            this.romanizeMemory.set(romKey, res);
          }
        }
        const latest = this.lyricsByTrack[trackId];
        if (romanized && latest && latest.lines.length === romanized.length) {
          const nextLines = latest.lines.map((l, idx) => ({
            ...l,
            romanized: l.romanized || romanized![idx] || undefined,
          }));
          this.lyricsByTrack = {
            ...this.lyricsByTrack,
            [trackId]: { ...latest, lines: nextLines },
          };
        }
      }
    }
  }

  async fetchLyricsForTrack(
    track: Track,
    opts?: { forceRefresh?: boolean; providerId?: string; durationOverrideSec?: number }
  ) {
    const force = Boolean(opts?.forceRefresh);
    const providerId = opts?.providerId ?? this.getEffectiveProviderId(track.id);
    const excludes = this.lyricsExcludedProviders[track.id] ?? [];
    if (!isTauriRuntime() || !track.id) return;
    if (!force && (this.lyricsByTrack[track.id] || this.lyricsInFlight.has(track.id))) {
      return;
    }

    void this.loadLyricsOffset(track.id);

    const liveDur =
      opts?.durationOverrideSec ??
      (playerStore.currentTrack?.id === track.id && playerStore.durationSec > 0
        ? playerStore.durationSec
        : track.durationSec);
    const durSec = liveDur > 0 ? Math.round(liveDur) : 0;

    const reqId = `${track.id}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    this.lyricsRequestIds[track.id] = reqId;
    this.lyricsInFlight.add(track.id);
    this.lyricsLoading = true;
    const nextErrs = { ...this.lyricsErrorByTrack };
    delete nextErrs[track.id];
    this.lyricsErrorByTrack = nextErrs;

    try {
      const res = await ipcInvoke<BackendLyricsResult>('lyrics_fetch', {
        title: track.title,
        artist: track.artist,
        album: track.album,
        durationSeconds: durSec > 0 ? durSec : null,
        forceRefresh: force,
        wordByWord: this.wordByWord,
        videoId: track.videoId || null,
        preferredProviderId: providerId,
        excludeProviderIds: excludes,
        trackId: track.id,
        requestId: reqId,
      });
      if (this.lyricsRequestIds[track.id] !== reqId) return;
      this.lyricsFetchedDurationSec = {
        ...this.lyricsFetchedDurationSec,
        [track.id]: durSec,
      };
      if (!res) return;
      this.applyBackendLyricsResult(track.id, res, providerId);
    } catch (err) {
      if (this.lyricsRequestIds[track.id] === reqId) {
        this.lyricsErrorByTrack = {
          ...this.lyricsErrorByTrack,
          [track.id]: err instanceof Error ? err.message : 'Lyrics unavailable',
        };
      }
    } finally {
      if (this.lyricsRequestIds[track.id] === reqId) {
        this.lyricsInFlight.delete(track.id);
        this.lyricsLoading = this.lyricsInFlight.size > 0;
      }
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

  setWordByWord(enabled: boolean) {
    this.wordByWord = enabled;
    settingsStore.saveBoolPref('lw_word_by_word', enabled);
    const cur = playerStore.currentTrack;
    if (cur) {
      void this.fetchLyricsForTrack(cur, { forceRefresh: true });
    }
  }

  setLyricsRomanized(enabled: boolean) {
    this.showRomanized = enabled;
    settingsStore.saveBoolPref('lw_lyrics_transliteration', enabled);
    const cur = playerStore.currentTrack;
    if (enabled && cur) {
      void this.enrichLyricsForTrack(cur.id);
    }
  }

  toggleLyricsTranslationQuick() {
    if (this.showTranslation && this.lyricsTranslationLang !== 'off') {
      this.setLyricsTranslationLang('off');
    } else {
      this.setLyricsTranslationLang(this.lyricsTranslationLang !== 'off' ? this.lyricsTranslationLang : 'en');
    }
  }

  setLyricsTranslationLang(lang: string) {
    this.lyricsTranslationLang = lang;
    this.showTranslation = lang !== 'off';
    settingsStore.saveStringPref('lw_lyrics_translation', lang);
    const cur = playerStore.currentTrack;
    if (cur && lang !== 'off') {
      void this.enrichLyricsForTrack(cur.id);
    }
  }

  setLyricsTextScale(scale: number) {
    this.lyricsFontScale = scale;
    settingsStore.saveDoublePref('lw_lyrics_text_scale', scale);
  }

  setLyricsAnimation(animation: string) {
    this.lyricsAnimation = animation;
    settingsStore.saveStringPref('lw_lyrics_animation', animation);
  }

  setLyricsAutoHideSeconds(seconds: number) {
    this.lyricsAutoHideSeconds = seconds;
    settingsStore.saveIntPref('lw_lyrics_autohide_seconds', seconds);
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
