import type {
  CustomAppRouteSlot,
  PlayerBarControlSlot,
  SidebarSlotItem,
  TrackContextMenuItemSlot,
} from './types';

class UiSlotsRegistry {
  sidebarItems = $state<SidebarSlotItem[]>([]);
  playerBarControls = $state<PlayerBarControlSlot[]>([]);
  trackContextMenuItems = $state<TrackContextMenuItemSlot[]>([]);
  customAppRoutes = $state<CustomAppRouteSlot[]>([]);

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

  clearForPlugin(pluginId: string) {
    this.sidebarItems = this.sidebarItems.filter((x) => x.pluginId !== pluginId);
    this.playerBarControls = this.playerBarControls.filter((x) => x.pluginId !== pluginId);
    this.trackContextMenuItems = this.trackContextMenuItems.filter((x) => x.pluginId !== pluginId);
    this.customAppRoutes = this.customAppRoutes.filter((x) => x.pluginId !== pluginId);
  }
}

export const uiSlots = new UiSlotsRegistry();
