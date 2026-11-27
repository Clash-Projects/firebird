import type {
  CustomAppRouteSlot,
  OverrideViewSlot,
  PlayerBarControlSlot,
  SidebarSlotItem,
  TrackContextMenuItemSlot,
} from './types';

function sanitizeSvg(raw: string): string {
  return raw
    .replace(/<script[\s\S]*?>[\s\S]*?<\/script>/gi, '')
    .replace(/\son[a-z]+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '');
}

class UiSlotsRegistry {
  sidebarItems = $state<SidebarSlotItem[]>([]);
  playerBarControls = $state<PlayerBarControlSlot[]>([]);
  trackContextMenuItems = $state<TrackContextMenuItemSlot[]>([]);
  customAppRoutes = $state<CustomAppRouteSlot[]>([]);
  nowPlayingOverride = $state<OverrideViewSlot | null>(null);
  playerBarOverride = $state<OverrideViewSlot | null>(null);
  titleBarOverride = $state<OverrideViewSlot | null>(null);
  sideRailOverride = $state<OverrideViewSlot | null>(null);
  iconPackPluginId = $state<string | null>(null);
  iconOverrides = $state<Record<string, string>>({});

  registerSidebarItem(item: SidebarSlotItem): () => void {
    this.sidebarItems = [...this.sidebarItems.filter((x) => x.id !== item.id), item];
    return () => {
      this.sidebarItems = this.sidebarItems.filter((x) => x.id !== item.id);
    };
  }

  registerPlayerBarControl(item: PlayerBarControlSlot): () => void {
    this.playerBarControls = [...this.playerBarControls.filter((x) => x.id !== item.id), item];
    return () => {
      this.playerBarControls = this.playerBarControls.filter((x) => x.id !== item.id);
    };
  }

  registerTrackMenuItem(item: TrackContextMenuItemSlot): () => void {
    this.trackContextMenuItems = [...this.trackContextMenuItems.filter((x) => x.id !== item.id), item];
    return () => {
      this.trackContextMenuItems = this.trackContextMenuItems.filter((x) => x.id !== item.id);
    };
  }

  registerCustomApp(item: CustomAppRouteSlot): () => void {
    this.customAppRoutes = [...this.customAppRoutes.filter((x) => x.route !== item.route), item];
    return () => {
      this.customAppRoutes = this.customAppRoutes.filter((x) => x.route !== item.route);
    };
  }

  registerNowPlayingView(item: OverrideViewSlot): () => void {
    this.nowPlayingOverride = item;
    return () => {
      if (this.nowPlayingOverride?.id === item.id) {
        this.nowPlayingOverride = null;
      }
    };
  }

  registerPlayerBar(item: OverrideViewSlot): () => void {
    this.playerBarOverride = item;
    return () => {
      if (this.playerBarOverride?.id === item.id) {
        this.playerBarOverride = null;
      }
    };
  }

  registerTitleBar(item: OverrideViewSlot): () => void {
    this.titleBarOverride = item;
    return () => {
      if (this.titleBarOverride?.id === item.id) {
        this.titleBarOverride = null;
      }
    };
  }

  registerSideRail(item: OverrideViewSlot): () => void {
    this.sideRailOverride = item;
    return () => {
      if (this.sideRailOverride?.id === item.id) {
        this.sideRailOverride = null;
      }
    };
  }

  registerIconPack(icons: Record<string, string>, pluginId: string): () => void {
    const clean: Record<string, string> = {};
    for (const [k, v] of Object.entries(icons || {})) {
      if (typeof v === 'string' && v.trim()) {
        clean[k] = sanitizeSvg(v.trim());
      }
    }
    this.iconPackPluginId = pluginId;
    this.iconOverrides = clean;
    return () => {
      if (this.iconPackPluginId === pluginId) {
        this.iconPackPluginId = null;
        this.iconOverrides = {};
      }
    };
  }

  clearForPlugin(pluginId: string) {
    this.sidebarItems = this.sidebarItems.filter((x) => x.pluginId !== pluginId);
    this.playerBarControls = this.playerBarControls.filter((x) => x.pluginId !== pluginId);
    this.trackContextMenuItems = this.trackContextMenuItems.filter((x) => x.pluginId !== pluginId);
    this.customAppRoutes = this.customAppRoutes.filter((x) => x.pluginId !== pluginId);
    if (this.nowPlayingOverride?.pluginId === pluginId) {
      this.nowPlayingOverride = null;
    }
    if (this.playerBarOverride?.pluginId === pluginId) {
      this.playerBarOverride = null;
    }
    if (this.titleBarOverride?.pluginId === pluginId) {
      this.titleBarOverride = null;
    }
    if (this.sideRailOverride?.pluginId === pluginId) {
      this.sideRailOverride = null;
    }
    if (this.iconPackPluginId === pluginId) {
      this.iconPackPluginId = null;
      this.iconOverrides = {};
    }
  }
}

export const uiSlots = new UiSlotsRegistry();
