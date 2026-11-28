import { loadTrackMetadata } from '$src/state/trackMetadata';
import type { Track } from '$src/state/types';

const visibilityCallbacks = new WeakMap<Element, (visible: boolean) => void>();
let observer: IntersectionObserver | undefined;

export function observeTrackMetadata(node: HTMLElement, track: Track | null) {
  let current = track;
  let cancel: (() => void) | undefined;
  let visible = typeof IntersectionObserver === 'undefined';

  function refresh() {
    cancel?.();
    cancel = visible && current ? loadTrackMetadata(current) : undefined;
  }

  if (visible) refresh();
  else {
    observer ??= new IntersectionObserver((entries) => {
      for (const entry of entries) visibilityCallbacks.get(entry.target)?.(entry.isIntersecting);
    });
    visibilityCallbacks.set(node, (next) => {
      visible = next;
      refresh();
    });
    observer.observe(node);
  }

  return {
    update(next: Track | null) {
      const changed = next?.videoId !== current?.videoId;
      current = next;
      if (changed) refresh();
    },
    destroy() {
      cancel?.();
      observer?.unobserve(node);
      visibilityCallbacks.delete(node);
    },
  };
}
