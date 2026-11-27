import { WaveState } from '$src/ui/theme/tokens';

export const LwInteraction = {
  hoverAlpha: WaveState.hoverAlpha,
  pressedAlpha: WaveState.pressedAlpha,
  focusRingPx: WaveState.focusRing,
  tooltipDelayMs: WaveState.tooltipDelayMs,
} as const;
