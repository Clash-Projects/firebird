import { isTauriRuntime } from '$src/lib/ipc';

class ImmersiveState {
  controlsIdle = $state(false);
  menuOpen = $state(false);
  isImmersive = $state(false);
  private timer: ReturnType<typeof setTimeout> | null = null;
  private idleDurationMs = 3500;

  ping() {
    this.controlsIdle = false;
    if (this.timer) clearTimeout(this.timer);
    if (this.menuOpen) return;
    this.timer = setTimeout(() => {
      if (!this.menuOpen) {
        this.controlsIdle = true;
      }
    }, this.idleDurationMs);
  }

  setMenuOpen(open: boolean) {
    this.menuOpen = open;
    if (open) {
      this.controlsIdle = false;
      if (this.timer) clearTimeout(this.timer);
    } else {
      this.ping();
    }
  }

  async setImmersive(next: boolean) {
    this.isImmersive = next;
    this.ping();
    if (isTauriRuntime()) {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        await getCurrentWindow().setFullscreen(next);
      } catch {
        // Ignore if window fullscreen is unavailable
      }
    } else if (typeof document !== 'undefined') {
      try {
        if (next && !document.fullscreenElement) {
          await document.documentElement.requestFullscreen();
        } else if (!next && document.fullscreenElement) {
          await document.exitFullscreen();
        }
      } catch {
        // Ignore
      }
    }
  }

  toggleImmersive() {
    void this.setImmersive(!this.isImmersive);
  }

  reset() {
    if (this.timer) clearTimeout(this.timer);
    this.controlsIdle = false;
    this.menuOpen = false;
  }
}

export const immersiveState = new ImmersiveState();
