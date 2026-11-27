import { cubicOut } from 'svelte/easing';
import { prefersReducedMotion } from 'svelte/motion';
import { fly } from 'svelte/transition';
import { settingsStore } from '$src/state/settings.svelte';
import { WaveMotion } from './tokens';

export const WAVE_ENTRANCE_STEP_MS = 36;
export const WAVE_ENTRANCE_ITEM_MS = 260;
export const WAVE_ENTRANCE_MAX_STAGGER = 4;
export const WAVE_ENTRANCE_RISE_PX = 9;

export function entranceDelayMs(index: number): number {
  return Math.min(Math.max(0, index), WAVE_ENTRANCE_MAX_STAGGER) * WAVE_ENTRANCE_STEP_MS;
}

export function waveSlide(node: Element, { x = 0, y = 0, duration = WaveMotion.normalMs }: {
  x?: number;
  y?: number;
  duration?: number;
}) {
  const reduced = settingsStore.reduceMotion || prefersReducedMotion.current;
  return fly(node, { x, y, duration: reduced ? 0 : duration, easing: cubicOut });
}
