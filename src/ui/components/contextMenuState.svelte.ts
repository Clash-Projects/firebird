import type { Track } from '$src/state/types';

export interface TrackContextMenuOptions {
  removeLabel?: string;
  onRemove?: () => void;
}

class ContextMenuState {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  track = $state<Track | null>(null);
  removeLabel = $state<string | undefined>(undefined);
  onRemove = $state<(() => void) | undefined>(undefined);
  propertiesTrack = $state<Track | null>(null);

  private getUiScale(): number {
    if (typeof document === 'undefined') return 1;
    const raw = getComputedStyle(document.documentElement).getPropertyValue('--lw-ui-scale');
    const parsed = parseFloat(raw);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
  }

  showForTrack(e: MouseEvent, track: Track, options?: TrackContextMenuOptions) {
    e.preventDefault();
    e.stopPropagation();
    const scale = this.getUiScale();
    const vw = window.innerWidth / scale;
    const vh = window.innerHeight / scale;
    const anchor =
      e.clientX === 0 && e.clientY === 0 && e.currentTarget instanceof HTMLElement
        ? e.currentTarget.getBoundingClientRect()
        : null;
    const rawX = anchor ? anchor.left / scale : e.clientX / scale;
    const rawY = anchor ? anchor.bottom / scale : e.clientY / scale;

    this.track = track;
    this.removeLabel = options?.removeLabel;
    this.onRemove = options?.onRemove;
    this.x = Math.max(8, Math.min(rawX, vw - 248));
    this.y = Math.max(8, Math.min(rawY, vh - 360));
    this.open = true;
  }

  clampToViewport(menuWidth: number, menuHeight: number) {
    if (!this.open || typeof window === 'undefined') return;
    const scale = this.getUiScale();
    const vw = window.innerWidth / scale;
    const vh = window.innerHeight / scale;
    const maxX = Math.max(8, vw - menuWidth - 8);
    const maxY = Math.max(8, vh - menuHeight - 8);
    if (this.x > maxX) this.x = maxX;
    if (this.y > maxY) this.y = maxY;
  }

  openProperties(track: Track) {
    this.propertiesTrack = track;
    this.open = false;
  }

  closeProperties() {
    this.propertiesTrack = null;
  }

  close() {
    this.open = false;
  }
}

export const contextMenu = new ContextMenuState();
