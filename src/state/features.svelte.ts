import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { settingsStore } from './settings.svelte';
import type { AudioDevice, LyricsDocument, PodcastShow, ToastMessage, Track } from './types';

export const LYRICS_PROVIDERS: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'auto', label: 'Auto (Word-First Racer)' },
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

export const EQ_FREQUENCIES = [
  25, 40, 63, 100, 160, 250, 400, 630, 1000, 1600, 2500, 4000, 6300, 10000, 16000,
];

export const EQ_PRESETS: Record<string, number[]> = {
  Flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  'Studio Reference': [0.5, 0.8, 0.4, 0, -0.3, -0.4, 0, 0.2, 0.5, 0.8, 1.0, 0.9, 0.6, 0.4, 0.2],
  'Warm Bass': [3.5, 4.0, 3.2, 2.0, 1.0, 0, -0.5, -0.4, 0, 0.4, 0.8, 1.0, 0.6, 0.3, 0],
  'Vocal Clarity': [-1.0, -0.5, 0, 0.5, 1.0, 1.8, 2.4, 2.8, 2.2, 1.5, 0.8, 0.4, 0, -0.4, -0.6],
  'Late Night': [1.5, 1.2, 0.8, 0, -0.5, -0.8, -0.5, 0, 0.5, 0.8, 0.4, -0.4, -1.0, -1.5, -2.0],
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
    publishedAt?: string;
    durationSeconds?: number;
    artworkUrl?: string;
  }>;
}

class FeaturesStore {
  eqEnabled = $state(false);
  eqPreset = $state('Flat');
  eqBands = $state<number[]>([...EQ_PRESETS.Flat]);
  eqPreampDb = $state(0);
  studioClarity = $state(false);
  headphoneProfile = $state('Off');

  devices = $state<AudioDevice[]>([]);
  selectedDeviceId = $state('');

  lyricsByTrack = $state<Record<string, LyricsDocument>>({});
  lyricsProviderId = $state('auto');
  wordByWord = $state(true);
  lyricsLoading = $state(false);
  lyricsOffsetMs = $state<Record<string, number>>({});
  showTranslation = $state(false);
  showRomanized = $state(false);
  lyricsFontScale = $state(1.0);

  podcasts = $state<PodcastShow[]>([]);
  podcastsLoading = $state(false);

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
      this.loadPodcasts(),
    ]);
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
    try {
      const shelves = await ipcInvoke<BackendPodcastShelf[]>('podcasts_home', { force });
      if (!Array.isArray(shelves)) return;
      const byId = new Map<string, PodcastShow>();
      for (const shelf of shelves) {
        for (const s of shelf.shows ?? []) {
          if (!s.showId || byId.has(s.showId)) continue;
          byId.set(s.showId, {
            id: s.showId,
            title: s.title,
            publisher: s.publisher || '',
            description: s.description || '',
            artworkUrl: s.artworkUrl || '',
            episodes: [],
          });
        }
        if (shelf.episodes && shelf.episodes.length > 0) {
          const shelfId = `shelf-${(shelf.title || 'episodes').toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
          const existing = byId.get(shelfId);
          const eps = shelf.episodes.map((ep) => ({
            id: ep.videoId,
            showId: ep.showId || shelfId,
            title: ep.title,
            description: ep.description || ep.showTitle || '',
            publishedAt: ep.publishedAt || '',
            durationSec: Number(ep.durationSeconds) || 0,
            resumeSec: 0,
            artworkUrl: ep.artworkUrl || '',
          }));
          if (existing) {
            existing.episodes = [...existing.episodes, ...eps];
          } else {
            byId.set(shelfId, {
              id: shelfId,
              title: shelf.title || 'Featured Episodes',
              publisher: 'YouTube Music Podcasts',
              description: `${eps.length} episodes`,
              artworkUrl: eps[0]?.artworkUrl || '',
              episodes: eps,
            });
          }
        }
      }
      this.podcasts = Array.from(byId.values());
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

  applyEqPreset(name: string) {
    const preset = EQ_PRESETS[name];
    if (!preset) return;
    this.eqPreset = name;
    this.eqBands = [...preset];
    void ipcInvoke('eq_set_preset', { presetId: name });
  }

  setEqBand(index: number, db: number) {
    const clamped = Math.max(-12, Math.min(12, Math.round(db * 10) / 10));
    this.eqBands[index] = clamped;
    this.eqPreset = 'Custom';
    void ipcInvoke('eq_set_band_gain', { bandIndex: index, gainDb: clamped });
  }

  getLyrics(trackId: string): LyricsDocument | null {
    return this.lyricsByTrack[trackId] ?? null;
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
