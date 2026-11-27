import type { PlayerEventType } from '$src/state/player.svelte';
import type { Track } from '$src/state/types';

export type PluginType = 'palette' | 'theme' | 'snippet' | 'widget';
export type PluginSource = 'official' | 'external';

export interface PluginAuthorObject {
  name: string;
  github: string;
}

export type PluginAuthor = PluginAuthorObject | string;

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  minAppVersion?: string;
  author: PluginAuthor;
  description: string;
  type: PluginType;
  entry: string;
  tags?: string[];
  previews?: string[];
  preview?: string;
  updatedAt?: string;
  source?: PluginSource;
  path?: string;
  repo?: string;
  commit?: string;
  subpath?: string;
  css?: string;
  icon?: string;
  route?: string;
  swatch?: [string, string, string];
  sourceUrl?: string;
}

export interface PluginRecord {
  manifest: PluginManifest;
  enabled: boolean;
  builtin?: boolean;
  sourceCode?: string;
  cssCode?: string;
  error?: string | null;
}

export interface MarketplacePluginEntry {
  id: string;
  name: string;
  type: PluginType;
  source: PluginSource;
  version: string;
  apiVersion: number;
  minAppVersion?: string;
  author: PluginAuthorObject;
  description: string;
  tags: string[];
  previews: string[];
  updatedAt: string;
  path?: string;
  entry?: string;
  repo?: string;
  commit?: string;
  subpath?: string;
  swatch?: [string, string, string];
}

export interface HydratedPluginDetails {
  manifest?: PluginManifest;
  readme?: string;
  paletteTokens?: Record<string, string>;
  loading?: boolean;
  error?: string;
}

export interface WidgetPlaybackSnapshot {
  currentTrack: {
    id: string;
    title: string;
    artist: string;
    album: string;
    artworkUrl: string;
    accentColor: string;
    codec: string;
    bitDepth: number;
    sampleRateHz: number;
    durationSec: number;
  } | null;
  isPlaying: boolean;
  positionMs: number;
  durationMs: number;
}

export interface SidebarSlotItem {
  id: string;
  pluginId: string;
  label: string;
  route: string;
  icon?: string;
}

export interface PlayerBarControlSlot {
  id: string;
  pluginId: string;
  label: string;
  icon?: string;
  onClick: () => void;
}

export interface TrackContextMenuItemSlot {
  id: string;
  pluginId: string;
  label: string;
  onClick: (track: Track) => void;
}

export interface CustomAppRouteSlot {
  id: string;
  pluginId: string;
  route: string;
  title: string;
  mount: (container: HTMLElement) => void | (() => void);
}

export interface ScopedPluginStorage {
  get<T = unknown>(key: string, fallback?: T): T | undefined;
  set(key: string, value: unknown): void;
  remove(key: string): void;
}

export interface SpectrumFrame {
  readonly bins: readonly number[];
  readonly rms: number;
}

export interface LastWaveAPI {
  readonly version: string;
  readonly player: {
    readonly play: () => void;
    readonly pause: () => void;
    readonly togglePlay: () => void;
    readonly next: () => void;
    readonly previous: () => void;
    readonly seek: (sec: number) => void;
    readonly setVolume: (val: number) => void;
    readonly getCurrentTrack: () => Track | null;
    readonly isPlaying: () => boolean;
    readonly getQueue: () => Track[];
    readonly getPosition: () => number;
    readonly onSpectrum: (fn: (frame: SpectrumFrame) => void) => () => void;
  };
  readonly events: {
    readonly on: (event: PlayerEventType, fn: (payload: unknown) => void) => () => void;
    readonly off: (event: PlayerEventType, fn: (payload: unknown) => void) => void;
  };
  readonly ui: {
    readonly registerSidebarItem: (item: Omit<SidebarSlotItem, 'pluginId'>, pluginId: string) => () => void;
    readonly registerPlayerBarControl: (item: Omit<PlayerBarControlSlot, 'pluginId'>, pluginId: string) => () => void;
    readonly registerTrackMenuItem: (item: Omit<TrackContextMenuItemSlot, 'pluginId'>, pluginId: string) => () => void;
    readonly registerCustomApp: (item: Omit<CustomAppRouteSlot, 'pluginId'>, pluginId: string) => () => void;
    readonly showToast: (title: string, subtitle?: string, severity?: 'info' | 'success' | 'warning' | 'error') => void;
  };
  readonly storage: (pluginId: string) => ScopedPluginStorage;
}

declare global {
  interface Window {
    LastWave?: LastWaveAPI;
  }
}
