export const CANVAS_ASPECT = 9 / 16;

export function canvasDimensionsForHeight(height: number): { width: number; height: number } {
  return { width: Math.round(height * CANVAS_ASPECT), height };
}
