import { ipcInvoke, ipcListen, isTauriRuntime } from '$src/lib/ipc';
import { MOCK_TRACKS } from './mockData';
import type { RepeatMode, Track } from './types';

interface BackendPlayerSnapshot {
  is_playing?: boolean;
  position_ms?: number;
  duration_ms?: number;
  volume?: number;
  shuffle?: boolean;
  repeat_mode?: string;
  current_index?: number;
}

export type PlayerEventType = 'trackChange' | 'playState' | 'progress' | 'queueUpdate';
type Listener<T = unknown> = (payload: T) => void;

class PlayerStore {
  queue = $state<Track[]>([...MOCK_TRACKS]);
  currentIndex = $state(0);
  isPlaying = $state(false);
  positionSec = $state(18);
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
  private ticker: ReturnType<typeof setInterval> | null = null;
  private listeners = new Map<PlayerEventType, Set<Listener>>();

  currentTrack = $derived<Track | null>(this.queue[this.currentIndex] ?? null);
  durationSec = $derived(this.currentTrack?.durationSec ?? 0);
  progress = $derived(this.durationSec > 0 ? Math.min(1, this.positionSec / this.durationSec) : 0);

  constructor() {
    this.startTicker();
  }

  async connectBackend(): Promise<() => void> {
    if (!isTauriRuntime()) return () => {};
    const applySnap = (snap: BackendPlayerSnapshot | null) => {
      if (!snap) return;
      if (typeof snap.is_playing === 'boolean') this.isPlaying = snap.is_playing;
      if (typeof snap.position_ms === 'number') this.positionSec = snap.position_ms / 1000;
      if (typeof snap.volume === 'number') this.volume = Math.max(0, Math.min(1, snap.volume / 100));
      if (typeof snap.shuffle === 'boolean') this.shuffle = snap.shuffle;
      if (typeof snap.current_index === 'number' && snap.current_index >= 0 && snap.current_index < this.queue.length) {
        this.currentIndex = snap.current_index;
      }
      if (snap.repeat_mode === 'off' || snap.repeat_mode === 'all' || snap.repeat_mode === 'one') {
        this.repeat = snap.repeat_mode;
      }
    };

    const initial = await ipcInvoke<BackendPlayerSnapshot>('player_snapshot');
    applySnap(initial);
    const unlisten = await ipcListen<BackendPlayerSnapshot>('player:snapshot', applySnap);
    return () => {
      unlisten?.();
    };
  }

  private startTicker() {
    if (this.ticker) clearInterval(this.ticker);
    this.ticker = setInterval(() => {
      if (!this.isPlaying || !this.currentTrack) return;
      const nextPos = this.positionSec + 0.25 * this.speed;
      if (nextPos >= this.currentTrack.durationSec) {
        if (this.repeat === 'one') {
          this.positionSec = 0;
        } else {
          this.next();
        }
      } else {
        this.positionSec = nextPos;
      }
      this.emit('progress', { positionSec: this.positionSec, durationSec: this.durationSec });
    }, 250);
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

  playTrack(track: Track, list?: Track[]) {
    if (list && list.length > 0) {
      this.queue = [...list];
      const idx = this.queue.findIndex((t) => t.id === track.id);
      this.currentIndex = idx >= 0 ? idx : 0;
      this.emit('queueUpdate', this.queue);
    } else {
      const idx = this.queue.findIndex((t) => t.id === track.id);
      if (idx >= 0) {
        this.currentIndex = idx;
      } else {
        this.queue = [track, ...this.queue];
        this.currentIndex = 0;
        this.emit('queueUpdate', this.queue);
      }
    }
    this.positionSec = 0;
    this.isPlaying = true;
    this.emit('trackChange', this.currentTrack);
    this.emit('playState', { isPlaying: true });
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
    this.positionSec = Math.max(0, Math.min(this.durationSec, sec));
    void ipcInvoke('player_seek', { positionMs: Math.round(this.positionSec * 1000) });
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
    void ipcInvoke('player_set_volume', { volume: Math.round(clamped * 100) });
  }

  toggleMute() {
    if (this.muted) {
      this.muted = false;
      this.volume = this.preMuteVolume || 0.7;
    } else {
      this.preMuteVolume = this.volume;
      this.muted = true;
      this.volume = 0;
    }
    void ipcInvoke('player_set_volume', { volume: Math.round(this.volume * 100) });
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
    this.emit('queueUpdate', this.queue);
  }

  addToQueue(track: Track) {
    this.queue = [...this.queue, track];
    this.emit('queueUpdate', this.queue);
  }

  removeFromQueue(index: number) {
    if (index < 0 || index >= this.queue.length) return;
    const copy = [...this.queue];
    copy.splice(index, 1);
    this.queue = copy;
    if (index < this.currentIndex) this.currentIndex -= 1;
    if (this.currentIndex >= this.queue.length) this.currentIndex = Math.max(0, this.queue.length - 1);
    this.emit('queueUpdate', this.queue);
  }

  moveQueueItem(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= this.queue.length || to >= this.queue.length) return;
    const copy = [...this.queue];
    const [item] = copy.splice(from, 1);
    copy.splice(to, 0, item);
    if (this.currentIndex === from) this.currentIndex = to;
    this.queue = copy;
    this.emit('queueUpdate', this.queue);
  }

  clearUpcoming() {
    this.queue = this.queue.slice(0, this.currentIndex + 1);
    this.emit('queueUpdate', this.queue);
  }
}

export const playerStore = new PlayerStore();

export function formatDuration(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}:${rem.toString().padStart(2, '0')}`;
}
