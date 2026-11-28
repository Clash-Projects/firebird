<script module lang="ts">
  export interface SelectOption<T extends string | number = string> {
    value: T;
    label: string;
    hint?: string;
    fontFamily?: string;
  }
</script>

<script lang="ts" generics="T extends string | number = string">
  import { tick } from 'svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    value: T;
    options: ReadonlyArray<SelectOption<T>>;
    label: string;
    placeholder?: string;
    disabled?: boolean;
    expanded?: boolean;
    minMenuWidth?: number;
    menuMaxWidth?: number;
    class?: string;
    onchange?: (value: T) => void;
  }

  let {
    value = $bindable(),
    options,
    label,
    placeholder = '',
    disabled = false,
    expanded = false,
    minMenuWidth = 200,
    menuMaxWidth = 360,
    class: className = '',
    onchange,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let open = $state(false);
  let highlightIdx = $state(-1);
  let menuTop = $state(0);
  let menuLeft = $state(0);
  let menuWidth = $state(200);
  let menuMaxH = $state(360);
  let openUpward = $state(false);

  let selectedOption = $derived(options.find((o) => o.value === value));
  let displayLabel = $derived(selectedOption?.label ?? placeholder);

  function getCoordScale(el: HTMLElement, rect: DOMRect): number {
    if (el.offsetWidth > 0 && rect.width > 0) {
      const ratio = rect.width / el.offsetWidth;
      if (Number.isFinite(ratio) && ratio > 0.25 && ratio < 4) return ratio;
    }
    return 1;
  }

  function updatePosition() {
    if (!triggerEl || !menuEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const scale = getCoordScale(triggerEl, rect);
    const vw = window.innerWidth / scale;
    const vh = window.innerHeight / scale;
    const tLeft = rect.left / scale;
    const tTop = rect.top / scale;
    const tBottom = rect.bottom / scale;
    const tWidth = rect.width / scale;

    const upper = Math.max(menuMaxWidth, tWidth);
    let longest = 0;
    for (const opt of options) {
      longest = Math.max(longest, opt.label.length * 7.6 + (opt.hint ? opt.hint.length * 6.4 + 20 : 0) + 56);
    }
    const computedW = Math.min(Math.max(longest, tWidth, minMenuWidth), upper, vw - 16);
    menuWidth = Math.round(computedW);

    const gap = 4;
    const margin = 8;
    const spaceBelow = vh - tBottom - margin - gap;
    const spaceAbove = tTop - margin - gap;
    const estHeight = Math.min(380, options.length * 34 + 10);

    if (spaceBelow < Math.min(estHeight, 200) && spaceAbove > spaceBelow) {
      openUpward = true;
      menuMaxH = Math.max(120, Math.min(380, Math.floor(spaceAbove)));
      const actualH = Math.min(menuEl.scrollHeight || estHeight, menuMaxH);
      menuTop = Math.max(margin, Math.round(tTop - gap - actualH));
    } else {
      openUpward = false;
      menuMaxH = Math.max(120, Math.min(380, Math.floor(spaceBelow)));
      menuTop = Math.round(tBottom + gap);
    }

    menuLeft = Math.round(Math.max(margin, Math.min(tLeft, vw - menuWidth - margin)));
  }

  async function openMenu() {
    if (disabled || open || options.length === 0) return;
    open = true;
    const idx = options.findIndex((o) => o.value === value);
    highlightIdx = idx >= 0 ? idx : 0;

    await tick();
    if (!menuEl) return;
    try {
      if (!menuEl.matches(':popover-open')) menuEl.showPopover();
    } catch {
      // Fallback when popover API is unavailable
    }
    updatePosition();
    await tick();
    updatePosition();
    scrollHighlightedIntoView();
    menuEl.focus({ preventScroll: true });
  }

  function closeMenu(restoreFocus = false) {
    if (!open) return;
    open = false;
    if (menuEl) {
      try {
        if (menuEl.matches(':popover-open')) menuEl.hidePopover();
      } catch {
        // Ignore
      }
    }
    if (restoreFocus) {
      triggerEl?.focus({ preventScroll: true });
    }
  }

  function toggleMenu() {
    if (open) closeMenu(true);
    else void openMenu();
  }

  function pickOption(opt: SelectOption<T>) {
    const changed = value !== opt.value;
    value = opt.value;
    closeMenu(true);
    if (changed) onchange?.(opt.value);
  }

  function scrollHighlightedIntoView() {
    if (!menuEl || highlightIdx < 0) return;
    const row = menuEl.querySelector<HTMLElement>(`[data-opt-idx="${highlightIdx}"]`);
    row?.scrollIntoView({ block: 'nearest' });
  }

  function onTriggerKeyDown(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      void openMenu();
    }
  }

  function onMenuKeyDown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape' || e.key === 'Tab') {
      e.preventDefault();
      closeMenu(true);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      highlightIdx = (highlightIdx + 1) % options.length;
      scrollHighlightedIntoView();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      highlightIdx = (highlightIdx - 1 + options.length) % options.length;
      scrollHighlightedIntoView();
      return;
    }
    if (e.key === 'Home') {
      e.preventDefault();
      highlightIdx = 0;
      scrollHighlightedIntoView();
      return;
    }
    if (e.key === 'End') {
      e.preventDefault();
      highlightIdx = options.length - 1;
      scrollHighlightedIntoView();
      return;
    }
    if ((e.key === 'Enter' || e.key === ' ') && highlightIdx >= 0 && highlightIdx < options.length) {
      e.preventDefault();
      pickOption(options[highlightIdx]);
    }
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (!open) return;
    const t = e.target as Node | null;
    if (t && (triggerEl?.contains(t) || menuEl?.contains(t))) return;
    closeMenu(false);
  }

  function onWindowScrollOrResize(e: Event) {
    if (!open) return;
    if (menuEl && e.target instanceof Node && menuEl.contains(e.target)) return;
    updatePosition();
  }
</script>

<svelte:window
  onpointerdown={onWindowPointerDown}
  onresize={onWindowScrollOrResize}
  onscrollcapture={onWindowScrollOrResize}
/>

<button
  bind:this={triggerEl}
  type="button"
  {disabled}
  aria-label={label}
  aria-haspopup="listbox"
  aria-expanded={open}
  onclick={toggleMenu}
  onkeydown={onTriggerKeyDown}
  data-open={open}
  class="lw-select-trigger {expanded ? 'w-full' : ''} {className}"
>
  <span
    class="lw-select-label truncate"
    style:font-family={selectedOption?.fontFamily || undefined}
  >
    {displayLabel}
  </span>
  <WaveIcon name="chevronDown" size={10} class="lw-select-chevron" />
</button>

{#if open}
  <div
    bind:this={menuEl}
    popover="manual"
    role="listbox"
    aria-label={label}
    tabindex="-1"
    onkeydown={onMenuKeyDown}
    class="lw-flyout-surface lw-select-flyout"
    class:upward={openUpward}
    style:left="{menuLeft}px"
    style:top="{menuTop}px"
    style:width="{menuWidth}px"
    style:max-height="{menuMaxH}px"
  >
    {#each options as opt, idx (opt.value)}
      {@const isSelected = opt.value === value}
      <button
        type="button"
        role="option"
        aria-selected={isSelected}
        data-opt-idx={idx}
        data-highlighted={highlightIdx === idx}
        onpointerenter={() => (highlightIdx = idx)}
        onclick={() => pickOption(opt)}
        class="lw-menu-row"
      >
        <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
          {#if isSelected}
            <WaveIcon name="check" size={13} />
          {/if}
        </span>
        <span
          class="min-w-0 flex-1 truncate"
          style:font-family={opt.fontFamily || undefined}
        >
          {opt.label}
        </span>
        {#if opt.hint}
          <span class="lw-menu-hint">{opt.hint}</span>
        {/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .lw-select-trigger {
    display: inline-flex;
    height: 36px;
    min-width: 148px;
    max-width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 14px;
    border: 1px solid var(--lw-outline);
    border-radius: 10px;
    background: var(--lw-surface-raised);
    color: var(--lw-text-primary);
    font-family: inherit;
    font-size: 13px;
    line-height: 1.2;
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--lw-motion-fast) var(--ease-native),
      border-color var(--lw-motion-fast) var(--ease-native);
  }

  .lw-select-trigger:not(:disabled):hover {
    background: var(--lw-surface-overlay);
    border-color: color-mix(in srgb, var(--lw-text-secondary) 36%, var(--lw-outline));
  }

  .lw-select-trigger[data-open="true"] {
    background: var(--lw-surface-overlay);
    border-color: var(--lw-outline);
  }

  .lw-select-trigger:focus-visible {
    border-color: var(--lw-accent);
    outline: none;
  }

  .lw-select-trigger:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .lw-select-label {
    min-width: 0;
    flex: 1;
  }

  .lw-select-chevron {
    flex-shrink: 0;
    color: var(--lw-text-secondary);
    transition: transform 120ms var(--ease-native);
  }

  .lw-select-Flyout,
  .lw-select-flyout {
    position: fixed;
    inset: auto;
    margin: 0;
    z-index: 9000;
    overflow-y: auto;
    overscroll-behavior: contain;
    animation: lw-flyout-in 90ms var(--ease-native);
  }

  .lw-select-flyout.upward {
    animation-name: lw-flyout-in-up;
  }

  @keyframes lw-flyout-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes lw-flyout-in-up {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
