import { featuresStore } from '$src/state/features.svelte';
import { playerStore, type PlayerEventType } from '$src/state/player.svelte';
import { createPluginStorage } from './storage';
import type {
  CustomAppRouteSlot,
  LastWaveAPI,
  PlayerBarControlSlot,
  SidebarSlotItem,
  TrackContextMenuItemSlot,
} from './types';
import { uiSlots } from './uiSlots.svelte';

let cachedApi: LastWaveAPI | null = null;

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
      getCurrentTrack: () => playerStore.currentTrack,
      isPlaying: () => playerStore.isPlaying,
      getQueue: () => [...playerStore.queue],
      getPosition: () => playerStore.positionSec,
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
