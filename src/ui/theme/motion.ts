import { cubicOut } from 'svelte/easing';
import { prefersReducedMotion } from 'svelte/motion';
import { fly } from 'svelte/transition';
import { settingsStore } from '$src/state/settings.svelte';
import { WaveMotion } from './tokens';

// Matches LastWave Desktop motion.dart (origin/HEAD smooth desktop transitions)
export const WAVE_ENTRANCE_STEP_MS = 25;
export const WAVE_ENTRANCE_ITEM_MS = 160;
export const WAVE_ENTRANCE_MAX_STAGGER = 4;
export const WAVE_ENTRANCE_RISE_PX = 8;

export function entranceDelayMs(index: number): number {
  return Math.min(Math.max(0, index), WAVE_ENTRANCE_MAX_STAGGER) * WAVE_ENTRANCE_STEP_MS;
}

function reduceMotion() {
  return settingsStore.reduceMotion || prefersReducedMotion.current;
}

export function waveSlide(node: Element, { x = 0, y = 0, opacity = 0, duration = WaveMotion.normalMs }: {
  x?: number;
  y?: number;
  opacity?: number;
  duration?: number;
}) {
  return fly(node, { x, y, opacity, duration: reduceMotion() ? 0 : duration, easing: cubicOut });
}

// The surface stays mounted so changing player modes does not reset its media or controls.
export function waveModeTransition(node: HTMLElement, mode: string) {
  let previousMode = mode;
  let animation: Animation | undefined;

  return {
    update(next: string) {
      if (next === previousMode) return;
      previousMode = next;
      const from = animation?.playState === 'running' ? getComputedStyle(node).opacity : 0.78;
      animation?.cancel();
      if (reduceMotion()) return;
      animation = node.animate([{ opacity: from }, { opacity: 1 }], {
        duration: WaveMotion.normalMs,
        easing: WaveMotion.easing,
      });
    },
    destroy() { animation?.cancel(); },
  };
}
