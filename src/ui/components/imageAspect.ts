export function aspectRatioClass(aspect: 'square' | 'wide' | 'portrait' = 'square'): string {
  if (aspect === 'wide') return 'aspect-video';
  if (aspect === 'portrait') return 'aspect-[9/16]';
  return 'aspect-square';
}
