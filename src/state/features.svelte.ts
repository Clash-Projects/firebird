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

  selectedDevice = $derived<AudioDevice | undefined>(
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

  getLyrics(trackId: string): LyricsDocument | null {
    return this.lyricsByTrack[trackId] ?? null;
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
