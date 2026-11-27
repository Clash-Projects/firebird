import type { Track } from '$src/state/types';

class ContextMenuState {
  open = $state(false);
  x = $state(0);
  y = $state(0);
  track = $state<Track | null>(null);

  showForTrack(e: MouseEvent, track: Track) {
    e.preventDefault();
    e.stopPropagation();
    this.track = track;
    const anchor = e.clientX === 0 && e.clientY === 0 && e.currentTarget instanceof HTMLElement
      ? e.currentTarget.getBoundingClientRect()
      : null;
    this.x = Math.max(8, Math.min(anchor?.left ?? e.clientX, window.innerWidth - 232));
    this.y = Math.max(8, Math.min(anchor?.bottom ?? e.clientY, window.innerHeight - 320));
    this.open = true;
  }

  close() {
    this.open = false;
  }
}

export const contextMenu = new ContextMenuState();
