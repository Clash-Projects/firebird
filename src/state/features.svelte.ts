import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { settingsStore } from './settings.svelte';
import type { AudioDevice, LyricsDocument, PodcastShow, ToastMessage, Track } from './types';

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
  isSynced?: boolean;
  isWordSynced?: boolean;
  source?: string;
}

interface BackendPodcastShelf {
  shows?: Array<{
    showId: string;
    title: string;
    publisher?: string;
    description?: string;
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
  showTranslation = $state(false);
  showRomanized = $state(false);
  lyricsFontScale = $state(1.0);

  podcasts = $state<PodcastShow[]>([]);

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

  async loadPodcasts() {
    if (!isTauriRuntime()) return;
    const shelves = await ipcInvoke<BackendPodcastShelf[]>('podcasts_home');
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
    }
    this.podcasts = Array.from(byId.values());
  }

  async fetchLyricsForTrack(track: Track) {
    if (!isTauriRuntime() || !track.id || this.lyricsByTrack[track.id] || this.lyricsInFlight.has(track.id)) {
      return;
    }
    this.lyricsInFlight.add(track.id);
    try {
      const res = await ipcInvoke<BackendLyricsResult>('lyrics_fetch', {
        title: track.title,
        artist: track.artist,
        album: track.album,
        durationSeconds: track.durationSec > 0 ? Math.round(track.durationSec) : null,
        forceRefresh: false,
        wordByWord: true,
        videoId: track.videoId || null,
        preferredProviderId: '',
        excludeProviderIds: [],
      });
      if (!res || !Array.isArray(res.lines) || res.lines.length === 0) return;

      this.lyricsByTrack = {
        ...this.lyricsByTrack,
        [track.id]: {
          trackId: track.id,
          provider: res.source || 'Lyrics',
          synced: Boolean(res.isSynced),
          wordSynced: Boolean(res.isWordSynced),
          lines: res.lines.map((l) => {
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
                      return { text: s.text || '', startMs: ws, endMs: ws + wd };
                    })
                  : undefined,
            };
          }),
        },
      };
    } finally {
      this.lyricsInFlight.delete(track.id);
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
    void ipcInvoke('jam_send_chat', { text: clean });
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
