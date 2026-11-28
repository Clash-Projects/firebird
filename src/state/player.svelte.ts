import { ipcInvoke, ipcListen, isTauriRuntime } from '$src/lib/ipc';
import type { RepeatMode, Track } from './types';

interface BackendPlayableTrack {
  title: string;
  artist: string;
  album: string;
  artworkUrl: string;
  videoId: string;
  playbackUrl: string;
  playbackMimeType: string;
  durationMs: number;
}

interface BackendPlayerSnapshot {
  current?: BackendPlayableTrack | null;
  queue?: BackendPlayableTrack[];
  currentIndex?: number;
  isPlaying?: boolean;
  isBuffering?: boolean;
  positionMs?: number;
  durationMs?: number;
  volume?: number;
  shuffleEnabled?: boolean;
  repeatMode?: RepeatMode;
  speed?: number;
  sleepMinutes?: number | null;
  error?: string | null;
}

function toPlayableTrack(track: Track): BackendPlayableTrack {
  return {
    title: track.title,
    artist: track.artist,
    album: track.album,
    artworkUrl: track.artworkUrl,
    videoId: track.videoId,
    playbackUrl: '',
    playbackMimeType: '',
    durationMs: Math.round(track.durationSec * 1000)
  };
}

function fromPlayableTrack(item: BackendPlayableTrack, fallbackIdx: number): Track {
  const id = item.videoId || item.playbackUrl || `track-${fallbackIdx}`;
  return {
    id,
    videoId: item.videoId,
    title: item.title || 'Unknown Title',
    artist: item.artist || 'Unknown Artist',
    artists: item.artist ? item.artist.split(/,\s*/).filter(Boolean) : ['Unknown Artist'],
    album: item.album || '',
    durationSec: Math.max(0, Math.round((item.durationMs || 0) / 1000)),
    artworkUrl: item.artworkUrl || '',
    accentColor: '#7c9bff',
    qualityTier: 'high',
    sampleRateHz: 48000,
    bitDepth: 16,
    codec: 'OPUS',
    bitrateKbps: 256,
    isLocal: Boolean(item.playbackUrl)
  };
}

export type PlayerEventType = 'trackChange' | 'playState' | 'progress' | 'queueUpdate';
type Listener<T = unknown> = (payload: T) => void;

class PlayerStore {
  queue = $state<Track[]>([]);
  currentIndex = $state(0);
  isPlaying = $state(false);
  isBuffering = $state(false);
  positionSec = $state(0);
  seekRevision = $state(0);
  backendDurationSec = $state(0);
  seeking = $state(false);
  lastError = $state<string | null>(null);
  volume = $state(0.82);
  muted = $state(false);
  shuffle = $state(false);
  repeat = $state<RepeatMode>('off');
  speed = $state(1.0);
  crossfadeSec = $state(4);
  automixEnabled = $state(true);
  skipSilence = $state(false);
  sleepTimerMin = $state<number | null>(null);
  private preMuteVolume = 0.82;
  private snapAnchorMs = 0;
  private snapAnchorTime = 0;
  private listeners = new Map<PlayerEventType, Set<Listener>>();

  currentTrack = $derived<Track | null>(this.queue[this.currentIndex] ?? null);
  durationSec = $derived(
    this.backendDurationSec > 0 ? this.backendDurationSec : (this.currentTrack?.durationSec ?? 0)
  );
  progress = $derived(this.durationSec > 0 ? Math.min(1, this.positionSec / this.durationSec) : 0);

  hydrateFromPrefs(all: Record<string, unknown> | null) {
    if (!all) return;
    if (typeof all.crossfadeEnabled === 'boolean') {
      if (!all.crossfadeEnabled) {
        this.crossfadeSec = 0;
      } else if (typeof all.crossfadeSeconds === 'number') {
        this.crossfadeSec = all.crossfadeSeconds;
      }
    }
    if (typeof all.autoMixEnabled === 'boolean') {
      this.automixEnabled = all.autoMixEnabled;
    }
  }

  async connectBackend(): Promise<() => void> {
    if (!isTauriRuntime()) return () => {};
    const applySnap = (snap: BackendPlayerSnapshot | null) => {
      if (!snap) return;
      if (Array.isArray(snap.queue) && snap.queue.length > 0) {
        const incoming = snap.queue.map(fromPlayableTrack);
        const sameQueue =
          incoming.length === this.queue.length &&
          incoming.every((t, i) => t.id === this.queue[i]?.id);
        if (!sameQueue) {
          this.queue = incoming;
        }
      }
      if (typeof snap.isPlaying === 'boolean') this.isPlaying = snap.isPlaying;
      if (typeof snap.isBuffering === 'boolean') this.isBuffering = snap.isBuffering;
      if (typeof snap.durationMs === 'number' && snap.durationMs > 0) {
        this.backendDurationSec = snap.durationMs / 1000;
      }
      if (typeof snap.positionMs === 'number') {
        this.snapAnchorMs = snap.positionMs;
        this.snapAnchorTime = performance.now();
        if (!this.seeking) {
          this.positionSec = snap.positionMs / 1000;
        }
      }
      if (typeof snap.volume === 'number') {
        this.volume = Math.max(0, Math.min(1, snap.volume));
        this.muted = this.volume === 0;
      }
      if (typeof snap.shuffleEnabled === 'boolean') this.shuffle = snap.shuffleEnabled;
      if (typeof snap.speed === 'number' && snap.speed > 0) this.speed = snap.speed;
      if (snap.sleepMinutes !== undefined) this.sleepTimerMin = snap.sleepMinutes;
      if (
        typeof snap.currentIndex === 'number' &&
        snap.currentIndex >= 0 &&
        snap.currentIndex < this.queue.length
      ) {
        this.currentIndex = snap.currentIndex;
      }
      if (snap.repeatMode === 'off' || snap.repeatMode === 'all' || snap.repeatMode === 'one') {
        this.repeat = snap.repeatMode;
      }
      if (snap.error) {
        this.lastError = snap.error;
      } else {
        this.lastError = null;
      }
    };

    let initial = await ipcInvoke<BackendPlayerSnapshot>('player_snapshot');
    if (!initial?.current && (!initial?.queue || initial.queue.length === 0)) {
      await ipcInvoke('player_restore_session');
      initial = await ipcInvoke<BackendPlayerSnapshot>('player_snapshot');
    }
    applySnap(initial);
    const unlisten = await ipcListen<BackendPlayerSnapshot>('player:snapshot', applySnap);

    // Smoothly interpolate the seekbar between 200ms backend ticks and re-sync periodically.
    const smoothTimer = setInterval(() => {
      if (!this.isPlaying || this.isBuffering || this.seeking || this.snapAnchorTime === 0) return;
      const elapsedSec = ((performance.now() - this.snapAnchorTime) / 1000) * this.speed;
      const next = this.snapAnchorMs / 1000 + elapsedSec;
      this.positionSec = this.durationSec > 0 ? Math.min(this.durationSec, next) : next;
    }, 100);

    const pollTimer = setInterval(() => {
      if (!this.isPlaying || this.seeking) return;
      void ipcInvoke<BackendPlayerSnapshot>('player_snapshot').then(applySnap);
    }, 1000);

    return () => {
      clearInterval(smoothTimer);
      clearInterval(pollTimer);
      unlisten?.();
    };
  }

  on(event: PlayerEventType, fn: Listener): () => void {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    set.add(fn);
    return () => set?.delete(fn);
  }

  off(event: PlayerEventType, fn: Listener) {
    this.listeners.get(event)?.delete(fn);
  }

  private emit(event: PlayerEventType, payload: unknown) {
    const set = this.listeners.get(event);
    if (!set) return;
    for (const fn of set) {
      try {
        fn(payload);
      } catch {
        // Isolate subscriber errors.
      }
    }
  }

  private resetPlaybackState(track: Track) {
    this.positionSec = 0;
    this.backendDurationSec = track.durationSec || 0;
    this.snapAnchorMs = 0;
    this.snapAnchorTime = 0;
    this.isPlaying = true;
    this.emit('trackChange', this.currentTrack);
    this.emit('playState', { isPlaying: true });
  }

  playTrack(track: Track, list?: Track[]) {
    if (list && list.length > 0) {
      this.queue = [...list];
      const idx = this.queue.findIndex((t) => t.id === track.id);
      this.currentIndex = idx >= 0 ? idx : 0;
      this.emit('queueUpdate', this.queue);
      void ipcInvoke('player_play_queue', {
        tracks: this.queue.map(toPlayableTrack),
        startIndex: this.currentIndex
      });
    } else {
      const idx = this.queue.findIndex((t) => t.id === track.id);
      if (idx >= 0) {
        this.currentIndex = idx;
      } else {
        this.queue = [track, ...this.queue];
        this.currentIndex = 0;
        this.emit('queueUpdate', this.queue);
      }
      void ipcInvoke('player_play', { track: toPlayableTrack(track) });
    }
    this.resetPlaybackState(track);
  }

  playRadio(track: Track, sourceLabel = 'Radio') {
    this.queue = [track];
    this.currentIndex = 0;
    this.emit('queueUpdate', this.queue);
    void ipcInvoke('player_play', {
      track: toPlayableTrack(track),
      sourceLabel,
      startRadio: true
    });
    this.resetPlaybackState(track);
  }

  togglePlay() {
    this.isPlaying = !this.isPlaying;
    void ipcInvoke('player_toggle');
    this.emit('playState', { isPlaying: this.isPlaying });
  }

  play() {
    if (this.isPlaying) return;
    this.isPlaying = true;
    void ipcInvoke('player_resume');
    this.emit('playState', { isPlaying: true });
  }

  pause() {
    if (!this.isPlaying) return;
    this.isPlaying = false;
    void ipcInvoke('player_pause');
    this.emit('playState', { isPlaying: false });
  }

  seek(sec: number) {
    const clamped = Math.max(0, Math.min(this.durationSec, sec));
    this.positionSec = clamped;
    this.seekRevision += 1;
    this.snapAnchorMs = Math.round(clamped * 1000);
    this.snapAnchorTime = performance.now();
    void ipcInvoke('player_seek', { positionMs: this.snapAnchorMs });
    this.emit('progress', { positionSec: this.positionSec, durationSec: this.durationSec });
  }

  next() {
    if (this.queue.length === 0) return;
    void ipcInvoke('player_next');
    if (this.shuffle && this.queue.length > 1) {
      let nextIdx = this.currentIndex;
      while (nextIdx === this.currentIndex) {
        nextIdx = Math.floor(Math.random() * this.queue.length);
      }
      this.currentIndex = nextIdx;
    } else if (this.currentIndex + 1 < this.queue.length) {
      this.currentIndex += 1;
    } else if (this.repeat === 'all') {
      this.currentIndex = 0;
    } else {
      this.isPlaying = false;
      this.emit('playState', { isPlaying: false });
      return;
    }
    this.positionSec = 0;
    this.emit('trackChange', this.currentTrack);
  }

  previous() {
    void ipcInvoke('player_previous');
    if (this.positionSec > 3) {
      this.seek(0);
      return;
    }
    if (this.queue.length === 0) return;
    this.currentIndex = this.currentIndex > 0 ? this.currentIndex - 1 : this.queue.length - 1;
    this.positionSec = 0;
    this.emit('trackChange', this.currentTrack);
  }

  setVolume(val: number) {
    const clamped = Math.max(0, Math.min(1, val));
    this.volume = clamped;
    this.muted = clamped === 0;
    if (clamped > 0) this.preMuteVolume = clamped;
    void ipcInvoke('player_set_volume', { volume: clamped, software: true });
  }

  toggleMute() {
    if (!this.muted) this.preMuteVolume = this.volume;
    this.setVolume(this.muted ? this.preMuteVolume || 0.7 : 0);
  }

  toggleShuffle() {
    this.shuffle = !this.shuffle;
    void ipcInvoke('player_toggle_shuffle');
  }

  cycleRepeat() {
    this.repeat = this.repeat === 'off' ? 'all' : this.repeat === 'all' ? 'one' : 'off';
    void ipcInvoke('player_cycle_repeat');
  }

  playNext(track: Track) {
    const copy = [...this.queue];
    copy.splice(this.currentIndex + 1, 0, track);
    this.queue = copy;
    void ipcInvoke('player_play_next', { track: toPlayableTrack(track) });
    this.emit('queueUpdate', this.queue);
  }

  addToQueue(track: Track) {
    this.queue = [...this.queue, track];
    void ipcInvoke('player_add_to_queue', { track: toPlayableTrack(track) });
    this.emit('queueUpdate', this.queue);
  }

  removeFromQueue(index: number) {
    if (index < 0 || index >= this.queue.length) return;
    const copy = [...this.queue];
    copy.splice(index, 1);
    this.queue = copy;
    if (index < this.currentIndex) this.currentIndex -= 1;
    if (this.currentIndex >= this.queue.length) this.currentIndex = Math.max(0, this.queue.length - 1);
    void ipcInvoke('player_remove_at', { index });
    this.emit('queueUpdate', this.queue);
  }

  moveQueueItem(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= this.queue.length || to >= this.queue.length) return;
    const copy = [...this.queue];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    if (this.currentIndex === from) this.currentIndex = to;
    this.queue = copy;
    void ipcInvoke('player_move_in_queue', { oldIndex: from, newIndex: to });
    this.emit('queueUpdate', this.queue);
  }

  clearUpcoming() {
    this.queue = this.queue.slice(0, this.currentIndex + 1);
    void ipcInvoke('player_clear_upcoming');
    this.emit('queueUpdate', this.queue);
  }

  cycleSpeed() {
    const steps = [0.75, 1.0, 1.25, 1.5, 2.0];
    const idx = steps.findIndex((s) => Math.abs(s - this.speed) < 0.05);
    const next = steps[(idx + 1) % steps.length];
    this.setSpeed(next);
  }

  setSpeed(nextSpeed: number) {
    const clamped = Math.max(0.5, Math.min(2.5, Math.round(nextSpeed * 100) / 100));
    this.speed = clamped;
    void ipcInvoke('player_set_speed', { speed: clamped });
  }

  setSleepTimer(minutes: number | null) {
    this.sleepTimerMin = minutes;
    void ipcInvoke('player_set_sleep_timer', { minutes });
  }
}

export const playerStore = new PlayerStore();

export function formatDuration(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}:${rem.toString().padStart(2, '0')}`;
}
