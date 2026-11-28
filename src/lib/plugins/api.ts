import { Channel } from '@tauri-apps/api/core';
import { router } from '$src/app/router.svelte';
import { toggleTrackLike } from '$src/app/trackActions';
import { windowState } from '$src/app/window.svelte';
import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { featuresStore } from '$src/state/features.svelte';
import { libraryStore } from '$src/state/library.svelte';
import { playerStore, type PlayerEventType } from '$src/state/player.svelte';
import { createPluginStorage } from './storage';
import type {
  CustomAppRouteSlot,
  LastWaveAPI,
  OverrideViewSlot,
  PlayerBarControlSlot,
  SidebarSlotItem,
  SpectrumFrame,
  TrackContextMenuItemSlot,
} from './types';
import { uiSlots } from './uiSlots.svelte';

let cachedApi: LastWaveAPI | null = null;
const spectrumListeners = new Set<(frame: SpectrumFrame) => void>();
let spectrumSubId: number | null = null;
let spectrumSubscribing = false;

function subscribeSpectrum(fn: (frame: SpectrumFrame) => void): () => void {
  spectrumListeners.add(fn);

  if (isTauriRuntime() && spectrumSubId === null && !spectrumSubscribing) {
    spectrumSubscribing = true;
    const channel = new Channel<SpectrumFrame>();
    channel.onmessage = (frame) => {
      for (const listener of spectrumListeners) {
        try {
          listener(frame);
        } catch {
          // Isolate plugin visualizer errors.
        }
      }
    };
    void ipcInvoke<number>('spectrum_subscribe', { channel }).then((id) => {
      spectrumSubscribing = false;
      if (spectrumListeners.size === 0) {
        if (typeof id === 'number') {
          void ipcInvoke('spectrum_unsubscribe', { subscriptionId: id });
        }
        return;
      }
      spectrumSubId = typeof id === 'number' ? id : null;
    });
  }

  return () => {
    spectrumListeners.delete(fn);
    if (spectrumListeners.size === 0 && spectrumSubId !== null) {
      const id = spectrumSubId;
      spectrumSubId = null;
      void ipcInvoke('spectrum_unsubscribe', { subscriptionId: id });
    }
  };
}

export function installLastWaveApi(): LastWaveAPI {
  if (cachedApi) return cachedApi;

  const api: LastWaveAPI = Object.freeze({
    version: '2.6.0',
    player: Object.freeze({
      play: () => playerStore.play(),
      pause: () => playerStore.pause(),
      togglePlay: () => playerStore.togglePlay(),
      next: () => playerStore.next(),
      previous: () => playerStore.previous(),
      seek: (sec: number) => playerStore.seek(sec),
      setVolume: (val: number) => playerStore.setVolume(val),
      toggleMute: () => playerStore.toggleMute(),
      isMuted: () => playerStore.muted,
      getVolume: () => playerStore.volume,
      toggleShuffle: () => playerStore.toggleShuffle(),
      getShuffle: () => playerStore.shuffle,
      cycleRepeat: () => playerStore.cycleRepeat(),
      getRepeat: () => playerStore.repeat,
      getCurrentTrack: () => playerStore.currentTrack,
      isPlaying: () => playerStore.isPlaying,
      isBuffering: () => playerStore.isBuffering,
      getQueue: () => [...playerStore.queue],
      getPosition: () => playerStore.positionSec,
      getDuration: () => playerStore.durationSec,
      onSpectrum: (fn: (frame: SpectrumFrame) => void) => subscribeSpectrum(fn),
    }),
    library: Object.freeze({
      isLiked: (trackId: string) => libraryStore.isLiked(trackId),
      toggleLike: (track = playerStore.currentTrack) => {
        if (track) toggleTrackLike(track);
      },
    }),
    nav: Object.freeze({
      navigate: (path: string) => router.navigate(path),
      back: () => router.back(),
      getPath: () => router.path,
    }),
    window: Object.freeze({
      toggleRail: () => windowState.toggleRail(),
      toggleMiniPlayer: () => windowState.toggleMiniPlayer(),
      toggleRightPanel: (panel: 'queue' | 'eq' | 'output') => windowState.toggleRightPanel(panel),
    }),
    events: Object.freeze({
      on: (event: PlayerEventType, fn: (payload: unknown) => void) => playerStore.on(event, fn),
      off: (event: PlayerEventType, fn: (payload: unknown) => void) => playerStore.off(event, fn),
    }),
    ui: Object.freeze({
      registerSidebarItem: (item: Omit<SidebarSlotItem, 'pluginId'>, pluginId: string) =>
        uiSlots.registerSidebarItem({ ...item, pluginId }),
      registerPlayerBarControl: (item: Omit<PlayerBarControlSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerPlayerBarControl({ ...item, pluginId }),
      registerTrackMenuItem: (item: Omit<TrackContextMenuItemSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerTrackMenuItem({ ...item, pluginId }),
      registerCustomApp: (item: Omit<CustomAppRouteSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerCustomApp({ ...item, pluginId }),
      registerNowPlayingView: (item: Omit<OverrideViewSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerNowPlayingView({ ...item, pluginId }),
      registerPlayerBar: (item: Omit<OverrideViewSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerPlayerBar({ ...item, pluginId }),
      registerTitleBar: (item: Omit<OverrideViewSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerTitleBar({ ...item, pluginId }),
      registerSideRail: (item: Omit<OverrideViewSlot, 'pluginId'>, pluginId: string) =>
        uiSlots.registerSideRail({ ...item, pluginId }),
      registerIconPack: (icons: Record<string, string>, pluginId: string) =>
        uiSlots.registerIconPack(icons, pluginId),
      showToast: (
        title: string,
        subtitle?: string,
        severity: 'info' | 'success' | 'warning' | 'error' = 'info'
      ) => featuresStore.showToast({ title, subtitle, severity }),
    }),
    storage: (pluginId: string) => createPluginStorage(pluginId),
  });

  window.LastWave = api;
  cachedApi = api;
  return api;
}
