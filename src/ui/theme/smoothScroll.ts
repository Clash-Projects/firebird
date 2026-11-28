/**
 * Desktop smooth wheel glide + horizontal mouse-drag scrolling, mirroring
 * WaveSmoothScroll / _GlideActivity from LastWave Desktop (lw_scrollbar.dart).
 *
 * Notched desktop mouse wheels emit coarse discrete steps; exponential target
 * chasing at rAF cadence turns successive wheel notches into one continuous
 * glide without restarting an ease curve. Trackpad fine deltas pass natively.
 */
interface ScrollOptions {
  axis?: 'y' | 'x';
  drag?: boolean;
  snap?: (projected: number, direction?: -1 | 0 | 1) => number;
}

export function waveSmoothScroll(node: HTMLElement, opts: ScrollOptions = {}) {
  let axis = opts.axis ?? 'y';
  let allowDrag = opts.drag ?? axis === 'x';
  let snap = opts.snap;

  const TAU_SEC = 0.085;
  let rafId = 0;
  let lastTime = 0;
  let target = axis === 'x' ? node.scrollLeft : node.scrollTop;
  let rawTarget = target;
  let glidePosition = target;
  let activeGlide = false;
  let dragging = false;
  let precisionTimer = 0;
  let manualScroll = false;

  const isReducedMotion = () =>
    document.documentElement.hasAttribute('data-reduced-motion') ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const updateSnap = () => {
    const scrolling = Boolean(snap) && (activeGlide || dragging || precisionTimer !== 0);
    if (scrolling === manualScroll) return;
    manualScroll = scrolling;
    node.toggleAttribute('data-manual-scroll', scrolling);
  };

  const stopGlide = () => {
    activeGlide = false;
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
    updateSnap();
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

  const startGlide = (nextTarget: number) => {
    const max = getMax();
    target = Math.max(0, Math.min(max, nextTarget));
    if (!activeGlide) {
      glidePosition = getCurrent();
      activeGlide = true;
      updateSnap();
      lastTime = 0;
      rafId = requestAnimationFrame(step);
    } else {
      updateSnap();
    }
  };

  const snapToCurrent = () => {
    if (!snap) {
      updateSnap();
      return;
    }
    const cur = getCurrent();
    const snapped = Math.max(0, Math.min(getMax(), snap(cur, 0)));
    if (isReducedMotion() || Math.abs(snapped - cur) < 0.6) {
      setCurrent(snapped);
      updateSnap();
      return;
    }
    startGlide(snapped);
  };

  const step = (now: number) => {
    const dt = lastTime ? Math.min(0.05, Math.max(0, (now - lastTime) / 1000)) : 0.016;
    lastTime = now;
    const max = getMax();
    target = Math.max(0, Math.min(max, target));
    // Keep fractional progress when the browser rounds scroll offsets to pixels.
    const current = snap ? glidePosition : getCurrent();
    const remaining = target - current;

    if (Math.abs(remaining) < 0.6) {
      setCurrent(target);
      stopGlide();
      return;
    }

    const delta = remaining * (1 - Math.exp(-dt / TAU_SEC));
    glidePosition = current + delta;
    setCurrent(glidePosition);
    rafId = requestAnimationFrame(step);
  };

  const onWheel = (e: WheelEvent) => {
    if (!e.cancelable || e.defaultPrevented || e.ctrlKey || e.metaKey) return;

    // Keep vertical wheel gestures available to the page when the pointer is over a shelf.
    const horizontal = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
    if (horizontal !== (axis === 'x')) {
      if (axis === 'x' && activeGlide && !snap) stopGlide();
      return;
    }

    const rawDelta = horizontal ? e.deltaX || e.deltaY : e.deltaY;
    if (rawDelta === 0) return;

    // Native diagonal scrolling can move both a shelf and its parent at once.
    const lockAxis = e.deltaX !== 0 && e.deltaY !== 0 && (
      axis === 'x' ||
      e.composedPath().some((element) => element instanceof Element && element.classList.contains('lw-scroll-x'))
    );
    const isNotchedWheel = e.deltaMode !== 0 || (Number.isInteger(rawDelta) && Math.abs(rawDelta) >= 40);
    const glide = isNotchedWheel && !isReducedMotion();
    if (!glide && !lockAxis) return;

    const pageSize = horizontal ? node.clientWidth : node.clientHeight;
    const pixelDelta =
      e.deltaMode === 1 ? rawDelta * 32 : e.deltaMode === 2 ? rawDelta * pageSize * 0.8 : rawDelta;

    const max = getMax();
    if (max <= 0 && !lockAxis) return;
    const cur = getCurrent();
    const atStart = cur <= 0.5;
    const atEnd = cur >= max - 0.5;
    if (!lockAxis && !activeGlide && ((pixelDelta < 0 && atStart) || (pixelDelta > 0 && atEnd))) {
      return; // Bubble to parent scrollable when already at boundary
    }

    e.preventDefault();
    if (!glide) {
      stopGlide();
      if (snap) {
        window.clearTimeout(precisionTimer);
        precisionTimer = window.setTimeout(() => {
          precisionTimer = 0;
          snapToCurrent();
        }, 140);
        updateSnap();
      }
      node.scrollBy(horizontal ? { left: pixelDelta, behavior: 'instant' } : { top: pixelDelta, behavior: 'instant' });
      return;
    }
    rawTarget = activeGlide ? Math.max(0, Math.min(max, rawTarget + pixelDelta)) : cur + pixelDelta;
    const dir = pixelDelta > 0 ? 1 : -1;
    const next = snap ? snap(rawTarget, dir) : rawTarget;
    startGlide(next);
  };

  // Optional mouse drag for horizontal shelves
  let moved = false;
  let capturedPointerId: number | null = null;
  let dragStartX = 0;
  let dragStartScroll = 0;
  let prevMoveX = 0;
  let prevMoveTime = 0;
  let velocityX = 0;

  const onPointerDown = (e: PointerEvent) => {
    stopGlide();
    if (!allowDrag || axis !== 'x' || e.button !== 0 || e.pointerType !== 'mouse') return;
    dragging = true;
    updateSnap();
    moved = false;
    capturedPointerId = null;
    dragStartX = e.clientX;
    dragStartScroll = node.scrollLeft;
    prevMoveX = e.clientX;
    prevMoveTime = performance.now();
    velocityX = 0;
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!dragging) return;
    const dx = e.clientX - dragStartX;
    if (!moved && Math.abs(dx) > 5) {
      moved = true;
      try {
        node.setPointerCapture(e.pointerId);
        capturedPointerId = e.pointerId;
      } catch {
        // Ignore if pointer cannot be captured
      }
    }
    if (moved) {
      const now = performance.now();
      const dtMs = now - prevMoveTime;
      if (dtMs > 0) {
        const instVel = ((prevMoveX - e.clientX) / dtMs) * 1000;
        velocityX = velocityX * 0.4 + instVel * 0.6;
        prevMoveX = e.clientX;
        prevMoveTime = now;
      }
      node.scrollLeft = dragStartScroll - dx;
    }
  };

  const onPointerUp = (e: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    if (capturedPointerId !== null) {
      try {
        if (node.hasPointerCapture(capturedPointerId)) {
          node.releasePointerCapture(capturedPointerId);
        }
      } catch {
        // Ignore
      }
      capturedPointerId = null;
    }
    if (moved) {
      const swallow = (ce: MouseEvent) => {
        ce.preventDefault();
        ce.stopPropagation();
      };
      node.addEventListener('click', swallow, { capture: true, once: true });
      setTimeout(() => node.removeEventListener('click', swallow, { capture: true }), 40);

      const stale = performance.now() - prevMoveTime > 90;
      if (!stale && !isReducedMotion() && Math.abs(velocityX) > 60) {
        const projected = Math.max(0, Math.min(getMax(), node.scrollLeft + velocityX * 0.14));
        const dir = velocityX > 0 ? 1 : -1;
        const next = snap ? snap(projected, dir) : projected;
        startGlide(next);
      } else {
        snapToCurrent();
      }
    } else {
      snapToCurrent();
    }
    moved = false;
    velocityX = 0;
  };

  node.addEventListener('wheel', onWheel, { passive: false });
  node.addEventListener('pointerdown', onPointerDown);
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('pointercancel', onPointerUp);

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
    update(next: ScrollOptions = {}) {
      axis = next.axis ?? 'y';
      allowDrag = next.drag ?? axis === 'x';
      snap = next.snap;
      updateSnap();
      updateThumb();
    },
    destroy() {
      snap = undefined;
      stopGlide();
      window.clearTimeout(precisionTimer);
      node.removeAttribute('data-manual-scroll');
      if (fadeTimer) clearTimeout(fadeTimer);
      node.removeEventListener('wheel', onWheel);
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('scroll', onNodeScroll);
      node.removeEventListener('pointermove', onNodeMove);
      thumb?.removeEventListener('pointerdown', onThumbDown);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerUp);
      window.removeEventListener('pointermove', onThumbMove);
      window.removeEventListener('pointerup', onThumbUp);
      resizeObs?.disconnect();
      mutationObs?.disconnect();
      thumb?.remove();
      thumb = null;
    },
  };
}
