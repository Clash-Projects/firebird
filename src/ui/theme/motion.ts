export const WAVE_ENTRANCE_STEP_MS = 20;
export const WAVE_ENTRANCE_ITEM_MS = 180;
export const WAVE_ENTRANCE_MAX_STAGGER = 6;
export const WAVE_ENTRANCE_RISE_PX = 14;

export function entranceDelayMs(index: number): number {
  return Math.min(Math.max(0, index), WAVE_ENTRANCE_MAX_STAGGER) * WAVE_ENTRANCE_STEP_MS;
}
