/**
 * Desktop smooth wheel glide + horizontal mouse-drag scrolling, mirroring
 * WaveSmoothScroll / _GlideActivity from LastWave Desktop (lw_scrollbar.dart).
 *
 * Notched desktop mouse wheels emit coarse discrete steps; exponential target
 * chasing at rAF cadence turns successive wheel notches into one continuous
 * glide without restarting an ease curve. Trackpad fine deltas pass natively.
 */
export function waveSmoothScroll(
  node: HTMLElement,
  opts: { axis?: 'y' | 'x'; drag?: boolean } = {}
) {
  let axis = opts.axis ?? 'y';
  let allowDrag = opts.drag ?? axis === 'x';

  const TAU_SEC = 0.085;
  let rafId = 0;
  let lastTime = 0;
  let target = axis === 'x' ? node.scrollLeft : node.scrollTop;
  let activeGlide = false;

  const stopGlide = () => {
    activeGlide = false;
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
  };

  const getMax = () =>
    axis === 'x'
      ? Math.max(0, node.scrollWidth - node.clientWidth)
      : Math.max(0, node.scrollHeight - node.clientHeight);

  const getCurrent = () => (axis === 'x' ? node.scrollLeft : node.scrollTop);

  const setCurrent = (v: number) => {
    if (axis === 'x') node.scrollLeft = v;
    else node.scrollTop = v;
  };

  const step = (now: number) => {
    const dt = lastTime ? Math.min(0.05, Math.max(0, (now - lastTime) / 1000)) : 0.016;
    lastTime = now;
    const max = getMax();
    target = Math.max(0, Math.min(max, target));
    const current = getCurrent();
    const remaining = target - current;

    if (Math.abs(remaining) < 0.6) {
      setCurrent(target);
      stopGlide();
      return;
    }

    const delta = remaining * (1 - Math.exp(-dt / TAU_SEC));
    setCurrent(current + delta);
    rafId = requestAnimationFrame(step);
  };

  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    if (document.documentElement.hasAttribute('data-reduced-motion')) return;

    const rawDelta =
      axis === 'x' ? (Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY) : e.deltaY;
    if (rawDelta === 0) return;

    // Fine trackpad precision gestures pass straight through natively.
    const isNotchedWheel = e.deltaMode !== 0 || (Number.isInteger(rawDelta) && Math.abs(rawDelta) >= 40);
    if (!isNotchedWheel) return;

    const pixelDelta =
      e.deltaMode === 1 ? rawDelta * 32 : e.deltaMode === 2 ? rawDelta * node.clientHeight * 0.8 : rawDelta;

    const max = getMax();
    if (max <= 0) return;
    const cur = getCurrent();
    const atStart = cur <= 0.5;
    const atEnd = cur >= max - 0.5;
    if (!activeGlide && ((pixelDelta < 0 && atStart) || (pixelDelta > 0 && atEnd))) {
      return; // Bubble to parent scrollable when already at boundary
    }

    e.preventDefault();
    if (!activeGlide) {
      target = cur + pixelDelta;
      activeGlide = true;
      lastTime = 0;
      rafId = requestAnimationFrame(step);
    } else {
      target = Math.max(0, Math.min(max, target + pixelDelta));
    }
  };

  // Optional mouse drag for horizontal shelves
  let dragging = false;
  let moved = false;
  let dragStartX = 0;
  let dragStartScroll = 0;

  const onPointerDown = (e: PointerEvent) => {
    stopGlide();
    if (!allowDrag || axis !== 'x' || e.button !== 0 || e.pointerType !== 'mouse') return;
    dragging = true;
    moved = false;
    dragStartX = e.clientX;
    dragStartScroll = node.scrollLeft;
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!dragging) return;
    const dx = e.clientX - dragStartX;
    if (Math.abs(dx) > 5) moved = true;
    if (moved) {
      node.scrollLeft = dragStartScroll - dx;
    }
  };

  const onPointerUp = () => {
    if (!dragging) return;
    dragging = false;
    if (moved) {
      const swallow = (ce: MouseEvent) => {
        ce.preventDefault();
        ce.stopPropagation();
      };
      node.addEventListener('click', swallow, { capture: true, once: true });
      setTimeout(() => node.removeEventListener('click', swallow, { capture: true }), 40);
    }
    moved = false;
  };

  node.addEventListener('wheel', onWheel, { passive: false });
  node.addEventListener('pointerdown', onPointerDown);
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);

  return {
    update(next: { axis?: 'y' | 'x'; drag?: boolean } = {}) {
      axis = next.axis ?? 'y';
      allowDrag = next.drag ?? axis === 'x';
    },
    destroy() {
      stopGlide();
      node.removeEventListener('wheel', onWheel);
      node.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
    },
  };
}
