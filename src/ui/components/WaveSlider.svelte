<script lang="ts">
  interface Props {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    label: string;
    valuetext?: string;
    orientation?: 'horizontal' | 'vertical';
    height?: number;
    class?: string;
    oninput?: (value: number) => void;
    onchange?: (value: number) => void;
  }

  let {
    value = $bindable(0),
    min = 0,
    max = 100,
    step = 1,
    disabled = false,
    label,
    valuetext,
    orientation = 'horizontal',
    height = 148,
    class: className = '',
    oninput,
    onchange,
  }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  let pct = $derived(
    max > min ? Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100)) : 0
  );

  $effect(() => {
    if (!inputEl) return;
    inputEl.style.setProperty('--attr-min', String(min));
    inputEl.style.setProperty('--attr-max', String(max));
    inputEl.style.setProperty('--control-value', String(value));
    inputEl.style.setProperty('--slider-progress', `${pct}%`);
  });

  function handleInput(e: Event) {
    const num = Number((e.currentTarget as HTMLInputElement).value);
    value = num;
    oninput?.(num);
  }

  function handleChange(e: Event) {
    const num = Number((e.currentTarget as HTMLInputElement).value);
    value = num;
    onchange?.(num);
  }
</script>

{#if orientation === 'vertical'}
  <div class="wave-vslider {disabled ? 'disabled' : ''} {className}" style:height="{height}px">
    <div class="vslider-track" aria-hidden="true">
      <div class="vslider-zero"></div>
      <div class="vslider-fill" style:height="{pct}%"></div>
    </div>
    <input
      bind:this={inputEl}
      type="range"
      {min}
      {max}
      {step}
      {disabled}
      {value}
      aria-label={label}
      aria-valuetext={valuetext}
      oninput={handleInput}
      onchange={handleChange}
      class="vslider-input"
      style:width="{height}px"
    />
    <div class="vslider-thumb" style:bottom="calc({pct}% - 5px)" aria-hidden="true"></div>
  </div>
{:else}
  <input
    bind:this={inputEl}
    type="range"
    {min}
    {max}
    {step}
    {disabled}
    {value}
    aria-label={label}
    aria-valuetext={valuetext}
    oninput={handleInput}
    onchange={handleChange}
    class="wave-slider {className}"
  />
{/if}

<style>
  .wave-slider {
    --slider-track-size: 3px;
    --slider-thumb-size: 10px;
    --slider-track-color: var(--lw-outline);
    --slider-fill-color: var(--lw-text-primary);
    --slider-thumb-color: var(--lw-text-primary);
    --slider-progress: 0%;
    appearance: none;
    inline-size: 100%;
    block-size: 20px;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: pointer;
  }

  .wave-slider:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .wave-slider::-webkit-slider-runnable-track {
    block-size: var(--slider-track-size);
    border: 0;
    border-radius: 999px;
    background: linear-gradient(
      to right,
      var(--slider-fill-color) var(--slider-progress),
      var(--slider-track-color) var(--slider-progress)
    );
  }

  .wave-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    inline-size: var(--slider-thumb-size);
    block-size: var(--slider-thumb-size);
    margin-block-start: calc((var(--slider-track-size) - var(--slider-thumb-size)) / 2);
    border: 0;
    border-radius: 50%;
    background: var(--slider-thumb-color);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
    transition: transform 110ms var(--ease-wave), background-color 110ms ease;
  }

  .wave-slider:not(:disabled):hover::-webkit-slider-thumb,
  .wave-slider:not(:disabled):active::-webkit-slider-thumb {
    transform: scale(1.2);
  }

  .wave-slider::-moz-range-track {
    block-size: var(--slider-track-size);
    border: 0;
    border-radius: 999px;
    background: var(--slider-track-color);
  }

  .wave-slider::-moz-range-progress {
    block-size: var(--slider-track-size);
    border-radius: 999px;
    background: var(--slider-fill-color);
  }

  .wave-slider::-moz-range-thumb {
    inline-size: var(--slider-thumb-size);
    block-size: var(--slider-thumb-size);
    border: 0;
    border-radius: 50%;
    background: var(--slider-thumb-color);
  }

  .wave-slider:focus-visible {
    outline: 2px solid var(--lw-focus-ring);
    outline-offset: 4px;
    border-radius: 4px;
  }

  /* Vertical EQ fader rack */
  .wave-vslider {
    position: relative;
    display: flex;
    width: 24px;
    align-items: center;
    justify-content: center;
  }

  .wave-vslider.disabled {
    opacity: 0.42;
  }

  .vslider-track {
    position: relative;
    width: 3px;
    height: 100%;
    overflow: hidden;
    border-radius: 999px;
    background: var(--lw-outline);
  }

  .vslider-zero {
    position: absolute;
    top: 50%;
    left: -3px;
    right: -3px;
    height: 1px;
    background: var(--lw-text-tertiary);
    opacity: 0.45;
  }

  .vslider-fill {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    border-radius: 999px;
    background: var(--lw-accent);
  }

  .vslider-thumb {
    position: absolute;
    left: 50%;
    width: 10px;
    height: 10px;
    transform: translateX(-50%);
    border-radius: 50%;
    background: var(--lw-text-primary);
    border: 1.5px solid var(--lw-surface);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.45);
    pointer-events: none;
    transition: transform 110ms var(--ease-wave);
  }

  .wave-vslider:not(.disabled):hover .vslider-thumb {
    transform: translateX(-50%) scale(1.18);
  }

  .vslider-input {
    position: absolute;
    height: 24px;
    margin: 0;
    opacity: 0;
    cursor: pointer;
    transform: rotate(-90deg);
  }

  .wave-vslider.disabled .vslider-input {
    cursor: default;
  }

  .wave-vslider:has(.vslider-input:focus-visible) {
    outline: 2px solid var(--lw-focus-ring);
    outline-offset: 2px;
    border-radius: 6px;
  }
</style>
