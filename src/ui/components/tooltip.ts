/**
 * Fluent 400ms desktop tooltip action replacing native HTML `title="..."` attributes.
 * Matches LWTooltip (WaveState.tooltipDelay = 400ms, WinUI dark surface + 6px radius).
 *
 * Uses top-layer `popover="hint"` where available (falling back cleanly to fixed overlay)
 * and never moves focus or triggers accidental activation.
 */

let sharedEl: HTMLDivElement | null = null;
let showTimer: ReturnType<typeof setTimeout> | null = null;
let activeTarget: HTMLElement | null = null;
let warmUntil = 0;

function ensureTooltipEl(): HTMLDivElement {
  if (sharedEl && document.body.contains(sharedEl)) return sharedEl;
  const el = document.createElement('div');
  el.className = 'lw-tooltip';
  el.setAttribute('role', 'tooltip');
  try {
    el.setAttribute('popover', 'hint');
  } catch {
    el.setAttribute('popover', 'manual');
  }
  document.body.appendChild(el);
  sharedEl = el;
  return el;
}

function hideTooltip() {
  if (showTimer) {
    clearTimeout(showTimer);
    showTimer = null;
  }
  activeTarget = null;
  if (!sharedEl) return;
  try {
    if (sharedEl.matches(':popover-open')) sharedEl.hidePopover();
  } catch {
    sharedEl.removeAttribute('data-open');
  }
}

function positionAndShow(target: HTMLElement, text: string) {
  if (!text.trim() || !target.isConnected) return;
  const el = ensureTooltipEl();
  el.textContent = text;
  activeTarget = target;
  warmUntil = performance.now() + 300;

  try {
    if (!el.matches(':popover-open')) el.showPopover();
  } catch {
    el.setAttribute('data-open', 'true');
  }

  const rect = target.getBoundingClientRect();
  const tip = el.getBoundingClientRect();
  const margin = 8;
  const gap = 6;

  // Prefer above when bottom edge is closer (player dock), else below (title bar)
  const spaceAbove = rect.top;
  const spaceBelow = window.innerHeight - rect.bottom;
  let top =
    rect.top > window.innerHeight * 0.65 || spaceAbove > tip.height + gap + 12 && spaceBelow < tip.height + gap + 12
      ? rect.top - tip.height - gap
      : rect.bottom + gap;

  top = Math.max(margin, Math.min(top, window.innerHeight - tip.height - margin));

  let left = rect.left + rect.width / 2 - tip.width / 2;
  left = Math.max(margin, Math.min(left, window.innerWidth - tip.width - margin));

  el.style.left = `${Math.round(left)}px`;
  el.style.top = `${Math.round(top)}px`;
}

export function waveTooltip(node: HTMLElement, text?: string | null) {
  let label = text ?? '';

  const scheduleShow = (immediate = false) => {
    if (!label.trim()) return;
    if (showTimer) clearTimeout(showTimer);
    const delay = immediate || performance.now() < warmUntil ? 60 : 400;
    showTimer = setTimeout(() => {
      showTimer = null;
      positionAndShow(node, label);
    }, delay);
  };

  const onEnter = () => scheduleShow(false);
  const onLeave = () => {
    if (activeTarget === node) warmUntil = performance.now() + 260;
    hideTooltip();
  };
  const onFocusIn = () => {
    if (node.matches(':focus-visible')) scheduleShow(false);
  };
  const onDown = () => hideTooltip();
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') hideTooltip();
  };

  node.addEventListener('pointerenter', onEnter);
  node.addEventListener('pointerleave', onLeave);
  node.addEventListener('focusin', onFocusIn);
  node.addEventListener('focusout', onLeave);
  node.addEventListener('pointerdown', onDown);
  node.addEventListener('keydown', onKey);

  return {
    update(next?: string | null) {
      label = next ?? '';
      if (activeTarget === node) {
        if (!label.trim()) hideTooltip();
        else positionAndShow(node, label);
      }
    },
    destroy() {
      if (activeTarget === node) hideTooltip();
      node.removeEventListener('pointerenter', onEnter);
      node.removeEventListener('pointerleave', onLeave);
      node.removeEventListener('focusin', onFocusIn);
      node.removeEventListener('focusout', onLeave);
      node.removeEventListener('pointerdown', onDown);
      node.removeEventListener('keydown', onKey);
    },
  };
}
