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
    this.x = Math.min(e.clientX, window.innerWidth - 230);
    this.y = Math.min(e.clientY, window.innerHeight - 320);
    this.open = true;
  }

  close() {
    this.open = false;
  }
}

export const contextMenu = new ContextMenuState();
