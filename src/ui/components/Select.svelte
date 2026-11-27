<script module lang="ts">
  export interface SelectOption<T extends string = string> {
    value: T;
    label: string;
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
  class="lw-select h-9 min-w-0 max-w-full cursor-pointer truncate rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3 text-[13px] leading-tight text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] {className}"
>
  {#each options as opt (opt.value)}
    <option value={opt.value}>{opt.label}</option>
  {/each}
</select>

<style>
  @supports (appearance: base-select) and selector(::picker(select)) {
    select, select::picker(select) { appearance: base-select; }

    select { align-items: center; gap: 10px; }
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
      inline-size: anchor-size(self-inline);
      min-inline-size: 0;
      max-block-size: var(--lw-select-picker-max-height, min(360px, 60dvh));
      margin-block: 4px;
      padding: 5px;
      border: 1px solid var(--lw-outline);
      border-radius: 12px;
      background: var(--lw-surface-raised);
      color: var(--lw-text-primary);
      box-shadow: var(--lw-shadow-flyout);
      overscroll-behavior: contain;
      scrollbar-width: thin;
      opacity: 0;
      transform: scale(calc(1 - 0.025 * var(--lw-motion-factor)));
      transform-origin: top right;
      transition: opacity var(--lw-motion-fast) var(--ease-wave), transform var(--lw-motion-fast) var(--ease-wave),
        display var(--lw-motion-fast), overlay var(--lw-motion-fast);
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
      border-radius: 7px;
      white-space: normal;
      overflow-wrap: anywhere;
      transition: background-color var(--lw-motion-fast);
    }

    option:checked { background: color-mix(in srgb, var(--lw-accent) 12%, transparent); font-weight: 650; }
    option:focus-visible { outline-offset: -2px; }

    @media (hover: hover) {
      select:not(:disabled):hover { border-color: color-mix(in srgb, var(--lw-text-secondary) 52%, var(--lw-outline)); }
      option:not(:checked):hover { background: var(--lw-wash-hover); }
    }
  }
</style>
