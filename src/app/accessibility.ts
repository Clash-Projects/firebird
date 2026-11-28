export function announcePolite(message: string) {
  let region = document.getElementById('lw-live-region');
  if (!region) {
    region = document.createElement('div');
    region.id = 'lw-live-region';
    region.setAttribute('aria-live', 'polite');
    region.setAttribute('aria-atomic', 'true');
    region.style.position = 'fixed';
    region.style.width = '1px';
    region.style.height = '1px';
    region.style.overflow = 'hidden';
    region.style.clipPath = 'inset(50%)';
    document.body.appendChild(region);
  }
  region.textContent = '';
  requestAnimationFrame(() => {
    if (region) region.textContent = message;
  });
}

export function dismissFlyout(node: HTMLElement, options: { open: boolean; close: () => void }) {
  function dismissOutside(event: Event) {
    const target = event instanceof FocusEvent ? event.relatedTarget : event.target;
    if (options.open && target instanceof Node && !node.contains(target)) options.close();
  }

  function dismissWithEscape(event: KeyboardEvent) {
    if (event.defaultPrevented || event.key !== 'Escape' || !options.open) return;
    event.preventDefault();
    options.close();
    if (node.contains(document.activeElement)) node.querySelector<HTMLButtonElement>('button[aria-expanded]')?.focus();
  }

  document.addEventListener('pointerdown', dismissOutside);
  document.addEventListener('keydown', dismissWithEscape);
  node.addEventListener('focusout', dismissOutside);

  return {
    update(next: typeof options) { options = next; },
    destroy() {
      document.removeEventListener('pointerdown', dismissOutside);
      document.removeEventListener('keydown', dismissWithEscape);
      node.removeEventListener('focusout', dismissOutside);
    },
  };
}

/**
 * Closes a modal `<dialog>` only when a primary click starts and ends outside its box,
 * preventing accidental dismissal when dragging text selections near dialog edges.
 */
export function dismissDialogOnBackdrop(node: HTMLDialogElement, onClose: () => void) {
  let pressedBackdrop = false;

  const isOutside = (e: MouseEvent) => {
    if (e.target !== node) return false;
    const rect = node.getBoundingClientRect();
    return (
      e.clientX < rect.left ||
      e.clientX >= rect.right ||
      e.clientY < rect.top ||
      e.clientY >= rect.bottom
    );
  };

  const onPointerDown = (e: PointerEvent) => {
    pressedBackdrop = e.button === 0 && isOutside(e);
  };
  const onPointerCancel = () => {
    pressedBackdrop = false;
  };
  const onClick = (e: MouseEvent) => {
    if (pressedBackdrop && isOutside(e)) onClose();
    pressedBackdrop = false;
  };

  node.addEventListener('pointerdown', onPointerDown);
  node.addEventListener('pointercancel', onPointerCancel);
  node.addEventListener('click', onClick);
  return {
    update(next: () => void) {
      onClose = next;
    },
    destroy() {
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('pointercancel', onPointerCancel);
      node.removeEventListener('click', onClick);
    },
  };
}

/**
 * Clamps a cascading submenu panel within its nearest clipping ancestor and the viewport
 * so submenus never render out of bounds or get clipped by parent containers.
 */
export function clampSubmenu(node: HTMLElement) {
  const PAD = 8;

  const adjust = () => {
    node.style.translate = '';
    const scrollTarget =
      node.firstElementChild instanceof HTMLElement ? node.firstElementChild : node;
    scrollTarget.style.maxHeight = '';

    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;

    const scale =
      node.offsetWidth > 0 && rect.width > 0 ? rect.width / node.offsetWidth : 1;

    let minX = PAD;
    let maxX = window.innerWidth - PAD;
    let minY = PAD;
    let maxY = window.innerHeight - PAD;

    let parent = node.parentElement;
    while (parent && parent !== document.body && parent !== document.documentElement) {
      const cs = getComputedStyle(parent);
      const clipsX = cs.overflowX === 'hidden' || cs.overflowX === 'clip' || cs.overflowX === 'auto' || cs.overflowX === 'scroll';
      const clipsY = cs.overflowY === 'hidden' || cs.overflowY === 'clip' || cs.overflowY === 'auto' || cs.overflowY === 'scroll';
      if (clipsX || clipsY) {
        const pr = parent.getBoundingClientRect();
        if (clipsX) {
          minX = Math.max(minX, pr.left + PAD);
          maxX = Math.min(maxX, pr.right - PAD);
        }
        if (clipsY) {
          minY = Math.max(minY, pr.top + PAD);
          maxY = Math.min(maxY, pr.bottom - PAD);
        }
      }
      parent = parent.parentElement;
    }

    const availH = Math.max(120, maxY - minY);
    if (rect.height > availH) {
      scrollTarget.style.maxHeight = `${Math.floor(availH / scale)}px`;
      scrollTarget.style.overflowY = 'auto';
    }

    const cur = node.getBoundingClientRect();
    let dx = 0;
    let dy = 0;

    if (cur.right > maxX) {
      dx = Math.max((minX - cur.left) / scale, (maxX - cur.right) / scale);
    } else if (cur.left < minX) {
      dx = Math.min((maxX - cur.right) / scale, (minX - cur.left) / scale);
    }

    if (cur.bottom > maxY) {
      dy = Math.max((minY - cur.top) / scale, (maxY - cur.bottom) / scale);
    } else if (cur.top < minY) {
      dy = Math.min((maxY - cur.bottom) / scale, (minY - cur.top) / scale);
    }

    if (dx !== 0 || dy !== 0) {
      node.style.translate = `${Math.round(dx)}px ${Math.round(dy)}px`;
    }
  };

  adjust();
  const ro = new ResizeObserver(adjust);
  ro.observe(node);
  window.addEventListener('resize', adjust);

  return {
    destroy() {
      ro.disconnect();
      window.removeEventListener('resize', adjust);
    },
  };
}
