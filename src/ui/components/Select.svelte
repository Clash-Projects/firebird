<script module lang="ts">
  export interface SelectOption<T extends string = string> {
    value: T;
    label: string;
    hint?: string;
    fontFamily?: string;
  }
</script>

<script lang="ts" generics="T extends string = string">
  interface Props {
    value: T;
    options: ReadonlyArray<SelectOption<T>>;
    label: string;
    class?: string;
    onchange?: (value: T) => void;
  }

  let { value = $bindable(), options, label, class: className = '', onchange }: Props = $props();
</script>

<select
  bind:value
  aria-label={label}
  onchange={(e) => onchange?.((e.currentTarget as HTMLSelectElement).value as T)}
  class="lw-select h-9 min-w-[160px] max-w-full cursor-pointer truncate rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3 text-[13px] leading-tight text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] {className}"
>
  <button type="button">
    <selectedcontent></selectedcontent>
  </button>
  {#each options as opt (opt.value)}
    <option value={opt.value} style:font-family={opt.fontFamily || undefined}>
      <span class="select-opt-row">
        <span class="select-opt-label">{opt.label}</span>
        {#if opt.hint}
          <span class="select-opt-hint">{opt.hint}</span>
        {/if}
      </span>
    </option>
  {/each}
</select>

<style>
  @supports (appearance: base-select) and selector(::picker(select)) {
    select,
    select::picker(select) {
      appearance: base-select;
    }

    select {
      align-items: center;
      justify-content: space-between;
      gap: 10px;
    }

    select::picker-icon {
      align-self: center;
      flex-shrink: 0;
      color: var(--lw-text-secondary);
      transition: transform var(--lw-motion-normal) var(--ease-wave), color var(--lw-motion-fast);
    }

    select:open::picker-icon {
      color: var(--lw-accent);
      transform: rotate(calc(180deg * var(--lw-motion-factor)));
    }

    select::picker(select) {
      box-sizing: border-box;
      min-inline-size: anchor-size(self-inline);
      max-inline-size: min(360px, 90vw);
      max-block-size: var(--lw-select-picker-max-height, min(360px, 60dvh));
      margin-block: 4px;
      padding: 4px;
      border: 1px solid var(--lw-outline);
      border-radius: 14px;
      background: var(--lw-surface-raised);
      color: var(--lw-text-primary);
      box-shadow: var(--lw-shadow-flyout);
      overscroll-behavior: contain;
      scrollbar-width: thin;
      opacity: 0;
      transform: scale(calc(1 - 0.025 * var(--lw-motion-factor)));
      transform-origin: top left;
      transition:
        opacity var(--lw-motion-fast) var(--ease-wave),
        transform var(--lw-motion-fast) var(--ease-wave),
        display var(--lw-motion-fast),
        overlay var(--lw-motion-fast);
      transition-behavior: allow-discrete;
    }

    select:open::picker(select) {
      opacity: 1;
      transform: none;
      transition-duration: var(--lw-motion-normal);
    }

    @starting-style {
      select:open::picker(select) {
        opacity: 0;
        transform: scale(calc(1 - 0.025 * var(--lw-motion-factor)));
      }
    }

    option {
      min-height: 32px;
      padding: 6px 10px;
      border-radius: 6px;
      white-space: nowrap;
      transition: background-color var(--lw-motion-fast);
    }

    .select-opt-row {
      display: flex;
      width: 100%;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }

    .select-opt-hint {
      font-size: 11px;
      color: var(--lw-text-tertiary);
    }

    option:checked {
      background: var(--lw-wash-pressed);
      font-weight: 600;
    }

    option:focus-visible {
      outline: 2px solid var(--lw-focus-ring);
      outline-offset: -2px;
    }

    @media (hover: hover) {
      select:not(:disabled):hover {
        border-color: color-mix(in srgb, var(--lw-text-secondary) 45%, var(--lw-outline));
      }
      option:not(:checked):hover {
        background: var(--lw-wash-hover);
      }
    }
  }
</style>
