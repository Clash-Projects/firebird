class WindowState {
  isMaximized = $state(false);
  isFullscreen = $state(false);
  isAlwaysOnTop = $state(false);
  isMiniPlayer = $state(false);
  railExpanded = $state(false);
  activeRightPanel = $state<'none' | 'queue' | 'lyrics' | 'canvas' | 'eq' | 'output'>('none');
  commandPaletteOpen = $state(false);
  updateDialogOpen = $state(false);
  welcomeOpen = $state(false);

  toggleRail() {
    this.railExpanded = !this.railExpanded;
  }

  toggleRightPanel(panel: 'queue' | 'lyrics' | 'canvas' | 'eq' | 'output') {
    this.activeRightPanel = this.activeRightPanel === panel ? 'none' : panel;
  }

  closeRightPanel() {
    this.activeRightPanel = 'none';
  }

  toggleMiniPlayer() {
    this.isMiniPlayer = !this.isMiniPlayer;
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
