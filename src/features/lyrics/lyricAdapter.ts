import type { LyricLine } from '$src/state/types';

export function activeLineIndex(lines: LyricLine[], positionMs: number): number {
  for (let i = lines.length - 1; i >= 0; i--) {
    if (positionMs >= lines[i].startMs) return i;
  }
  return 0;
}

export function wordProgress(startMs: number, endMs: number, positionMs: number): number {
  if (positionMs <= startMs) return 0;
  if (positionMs >= endMs) return 1;
  const span = Math.max(1, endMs - startMs);
  return (positionMs - startMs) / span;
}
