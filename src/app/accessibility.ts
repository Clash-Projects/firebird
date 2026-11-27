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
