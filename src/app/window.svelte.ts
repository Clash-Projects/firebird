import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import { featuresStore } from '$src/state/features.svelte';
import { playerStore } from '$src/state/player.svelte';
import { settingsStore, type MiniPlayerStyle } from '$src/state/settings.svelte';

export function miniPlayerBounds(
  style: MiniPlayerStyle,
  lyricsWide = false
): { width: number; height: number; minWidth: number; minHeight: number } {
  switch (style) {
    case 'compact':
      return { width: 320, height: 56, minWidth: 260, minHeight: 48 };
    case 'card':
      return { width: 280, height: 280, minWidth: 220, minHeight: 220 };
    case 'ambient':
      return { width: 440, height: 120, minWidth: 360, minHeight: 108 };
    case 'lyrics':
      return lyricsWide
        ? { width: 480, height: 148, minWidth: 380, minHeight: 130 }
        : { width: 440, height: 120, minWidth: 360, minHeight: 108 };
    case 'classic':
    default:
      return { width: 380, height: 78, minWidth: 300, minHeight: 64 };
  }
}

class WindowState {
  isMaximized = $state(false);
  isFullscreen = $state(false);
  isAlwaysOnTop = $state(false);
  isMiniPlayer = $state(false);
  railExpanded = $state(false);
  activeRightPanel = $state<'none' | 'queue' | 'eq' | 'output'>('none');
  commandPaletteOpen = $state(false);
  updateDialogOpen = $state(false);
  welcomeOpen = $state(false);

  toggleRail() {
    this.railExpanded = !this.railExpanded;
  }

  toggleRightPanel(panel: 'queue' | 'eq' | 'output') {
    this.activeRightPanel = this.activeRightPanel === panel ? 'none' : panel;
  }

  closeRightPanel() {
    this.activeRightPanel = 'none';
  }

  private hasSyncedLyricsNow(): boolean {
    const id = playerStore.currentTrack?.id;
    if (!id) return false;
    const doc = featuresStore.getLyrics(id);
    return Boolean(doc && doc.synced && doc.lines.length > 0);
  }

  toggleMiniPlayer() {
    const next = !this.isMiniPlayer;
    this.isMiniPlayer = next;
    this.isAlwaysOnTop = next;
    if (!isTauriRuntime()) return;
    const bounds = miniPlayerBounds(settingsStore.miniPlayerStyle, this.hasSyncedLyricsNow());
    void ipcInvoke('window_set_mini_mode', {
      enabled: next,
      width: bounds.width,
      height: bounds.height,
      minWidth: bounds.minWidth,
      minHeight: bounds.minHeight,
    });
  }

  syncMiniPlayerBounds(lyricsWide = this.hasSyncedLyricsNow()) {
    if (!this.isMiniPlayer || !isTauriRuntime()) return;
    const bounds = miniPlayerBounds(settingsStore.miniPlayerStyle, lyricsWide);
    void ipcInvoke('window_set_mini_mode', {
      enabled: true,
      width: bounds.width,
      height: bounds.height,
      minWidth: bounds.minWidth,
      minHeight: bounds.minHeight,
    });
  }

  toggleFullscreen() {
    this.isFullscreen = !this.isFullscreen;
    if (typeof document !== 'undefined') {
      if (this.isFullscreen && !document.fullscreenElement) {
        document.documentElement.requestFullscreen?.().catch(() => {});
      } else if (!this.isFullscreen && document.fullscreenElement) {
        document.exitFullscreen?.().catch(() => {});
      }
    }
  }
}

export const windowState = new WindowState();
