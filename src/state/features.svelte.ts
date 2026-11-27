import { MOCK_AUDIO_DEVICES, MOCK_LYRICS, MOCK_PODCASTS } from './mockData';
import type { AudioDevice, LyricsDocument, PodcastShow, ToastMessage } from './types';

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

class FeaturesStore {
  // Equalizer
  eqEnabled = $state(true);
  eqPreset = $state('Studio Reference');
  eqBands = $state<number[]>([...EQ_PRESETS['Studio Reference']]);
  eqPreampDb = $state(-1.0);
  studioClarity = $state(true);
  headphoneProfile = $state('Off');

  // Audio Output
  devices = $state<AudioDevice[]>([...MOCK_AUDIO_DEVICES]);
  selectedDeviceId = $state('dev-fiio-k7');

  // Lyrics
  lyricsByTrack = $state<Record<string, LyricsDocument>>({ ...MOCK_LYRICS });
  showTranslation = $state(false);
  showRomanized = $state(false);
  lyricsFontScale = $state(1.0);

  // Podcasts
  podcasts = $state<PodcastShow[]>([...MOCK_PODCASTS]);

  // Jam
  jamConnected = $state(false);
  jamRoomCode = $state('WAVE-7429');
  jamIsHost = $state(true);
  jamParticipants = $state([
    { id: 'u1', name: 'Ajisth (Host)', role: 'host' },
    { id: 'u2', name: 'Mira', role: 'listener' },
    { id: 'u3', name: 'Devon', role: 'co-host' },
  ]);
  jamChat = $state<JamChatMessage[]>([
    { id: 'm1', sender: 'Mira', text: 'This 24-bit master sounds super clean.', time: '01:12' },
    { id: 'm2', sender: 'Devon', text: 'Queue up Chrome Rain next!', time: '01:14' },
  ]);

  // Toasts / Infobar
  toasts = $state<ToastMessage[]>([]);

  selectedDevice = $derived(
    this.devices.find((d) => d.id === this.selectedDeviceId) ?? this.devices[0]
  );

  applyEqPreset(name: string) {
    const preset = EQ_PRESETS[name];
    if (!preset) return;
    this.eqPreset = name;
    this.eqBands = [...preset];
  }

  setEqBand(index: number, db: number) {
    this.eqBands[index] = Math.max(-12, Math.min(12, Math.round(db * 10) / 10));
    this.eqPreset = 'Custom';
  }

  getLyrics(trackId: string): LyricsDocument {
    return (
      this.lyricsByTrack[trackId] ?? {
        trackId,
        provider: 'LRCLIB (Synced)',
        synced: true,
        wordSynced: false,
        lines: MOCK_LYRICS['trk-01'].lines,
      }
    );
  }

  sendJamMessage(text: string) {
    const clean = text.trim();
    if (!clean) return;
    const now = new Date();
    const time = `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}`;
    this.jamChat = [...this.jamChat, { id: `m-${Date.now()}`, sender: 'You', text: clean, time }];
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
