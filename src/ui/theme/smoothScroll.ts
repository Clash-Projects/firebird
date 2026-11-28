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

  // Auto-hiding Fluent overlay scrollbar (matches WaveThinScrollbar: 6px, fades after 800ms idle)
  let thumb: HTMLDivElement | null = null;
  let fadeTimer: ReturnType<typeof setTimeout> | null = null;
  let thumbDragging = false;
  let thumbDragStartY = 0;
  let thumbDragStartScroll = 0;
  let resizeObs: ResizeObserver | null = null;
  let mutationObs: MutationObserver | null = null;

  const updateThumb = () => {
    if (!thumb) return;
    const ch = node.clientHeight;
    const sh = node.scrollHeight;
    if (axis !== 'y' || !node.classList.contains('lw-scroll') || sh <= ch + 2 || ch <= 0) {
      thumb.style.display = 'none';
      return;
    }
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      thumb.style.display = 'none';
      return;
    }
    thumb.style.display = 'block';
    const pad = 4;
    const trackH = Math.max(1, ch - pad * 2);
    const thumbH = Math.max(40, Math.round((ch / sh) * trackH));
    const maxScroll = Math.max(1, sh - ch);
    const ratio = Math.min(1, Math.max(0, node.scrollTop / maxScroll));
    const top = rect.top + pad + ratio * Math.max(0, trackH - thumbH);
    thumb.style.height = `${thumbH}px`;
    thumb.style.top = `${Math.round(top)}px`;
    thumb.style.right = `${Math.round(window.innerWidth - rect.right + 4)}px`;
  };

  const revealThumb = () => {
    if (!thumb) return;
    updateThumb();
    if (thumb.style.display === 'none') return;
    thumb.setAttribute('data-visible', 'true');
    if (fadeTimer) clearTimeout(fadeTimer);
    if (!thumbDragging) {
      fadeTimer = setTimeout(() => {
        thumb?.setAttribute('data-visible', 'false');
      }, 800);
    }
  };

  const onNodeScroll = () => revealThumb();
  const onNodeMove = (e: PointerEvent) => {
    if (!thumb || axis !== 'y') return;
    const rect = node.getBoundingClientRect();
    if (rect.right - e.clientX <= 22 && e.clientX <= rect.right) {
      revealThumb();
    }
  };

  const onThumbDown = (e: PointerEvent) => {
    if (e.button !== 0 || !thumb) return;
    e.preventDefault();
    e.stopPropagation();
    stopGlide();
    thumbDragging = true;
    thumbDragStartY = e.clientY;
    thumbDragStartScroll = node.scrollTop;
    thumb.setAttribute('data-dragging', 'true');
    if (fadeTimer) clearTimeout(fadeTimer);
  };

  const onThumbMove = (e: PointerEvent) => {
    if (!thumbDragging || !thumb) return;
    const ch = node.clientHeight;
    const sh = node.scrollHeight;
    const trackH = Math.max(1, ch - 8);
    const thumbH = Math.max(40, Math.round((ch / sh) * trackH));
    const travel = Math.max(1, trackH - thumbH);
    const maxScroll = Math.max(1, sh - ch);
    const dy = e.clientY - thumbDragStartY;
    node.scrollTop = Math.max(0, Math.min(maxScroll, thumbDragStartScroll + (dy / travel) * maxScroll));
    updateThumb();
  };

  const onThumbUp = () => {
    if (!thumbDragging) return;
    thumbDragging = false;
    thumb?.removeAttribute('data-dragging');
    revealThumb();
  };

  if (axis === 'y' && node.classList.contains('lw-scroll')) {
    thumb = document.createElement('div');
    thumb.className = 'lw-overlay-thumb';
    thumb.setAttribute('aria-hidden', 'true');
    document.body.appendChild(thumb);

    node.addEventListener('scroll', onNodeScroll, { passive: true });
    node.addEventListener('pointermove', onNodeMove, { passive: true });
    thumb.addEventListener('pointerdown', onThumbDown);
    window.addEventListener('pointermove', onThumbMove);
    window.addEventListener('pointerup', onThumbUp);
    resizeObs = new ResizeObserver(updateThumb);
    resizeObs.observe(node);
    mutationObs = new MutationObserver(updateThumb);
    mutationObs.observe(node, { attributes: true, attributeFilter: ['class'] });
  }

  return {
    update(next: { axis?: 'y' | 'x'; drag?: boolean } = {}) {
      axis = next.axis ?? 'y';
      allowDrag = next.drag ?? axis === 'x';
      updateThumb();
    },
    destroy() {
      stopGlide();
      if (fadeTimer) clearTimeout(fadeTimer);
      node.removeEventListener('wheel', onWheel);
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('scroll', onNodeScroll);
      node.removeEventListener('pointermove', onNodeMove);
      thumb?.removeEventListener('pointerdown', onThumbDown);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointermove', onThumbMove);
      window.removeEventListener('pointerup', onThumbUp);
      resizeObs?.disconnect();
      mutationObs?.disconnect();
      thumb?.remove();
      thumb = null;
    },
  };
}
