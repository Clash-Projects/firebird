import { router } from '$src/app/router.svelte';
import { windowState } from '$src/app/window.svelte';
import { playerStore } from '$src/state/player.svelte';

export function attachWaveHotkeys(): () => void {
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.defaultPrevented) return;
    const target = e.target instanceof HTMLElement ? e.target : null;
    const isInput =
      target &&
      (target.matches('input, textarea, select') || target.isContentEditable);

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      windowState.commandPaletteOpen = !windowState.commandPaletteOpen;
      return;
    }

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      router.navigate('/search');
      return;
    }

    if (isInput) return;

    if (e.code === 'Space') {
      if (target?.closest('button, a[href], [role="button"]')) return;
      e.preventDefault();
      if (playerStore.currentTrack) playerStore.togglePlay();
    } else if (e.key === 'ArrowRight' && e.altKey) {
      router.forward();
    } else if (e.key === 'ArrowLeft' && e.altKey) {
      router.back();
    } else if (e.key === 'ArrowRight') {
      playerStore.seek(playerStore.positionSec + 5);
    } else if (e.key === 'ArrowLeft') {
      playerStore.seek(playerStore.positionSec - 5);
    } else if (e.key.toLowerCase() === 'm') {
      playerStore.toggleMute();
    } else if (e.key.toLowerCase() === 'q') {
      windowState.toggleRightPanel('queue');
    } else if (e.key.toLowerCase() === 'l') {
      windowState.toggleRightPanel('lyrics');
    }
  };

  window.addEventListener('keydown', onKeyDown);
  return () => window.removeEventListener('keydown', onKeyDown);
}
