<script lang="ts">
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    value: string;
    placeholder?: string;
    ariaLabel?: string;
    type?: string;
    disabled?: boolean;
    prefixIcon?: WaveIconName;
    class?: string;
    autofocus?: boolean;
    oninput?: (value: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    ariaLabel,
    type = 'text',
    disabled = false,
    prefixIcon,
    class: className = '',
    autofocus = false,
    oninput,
    onkeydown,
  }: Props = $props();
</script>

{#if prefixIcon}
  <div class="relative flex w-full items-center">
    <WaveIcon
      name={prefixIcon}
      size={13}
      class="pointer-events-none absolute left-3 text-[var(--lw-text-tertiary)]"
    />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      {type}
      {placeholder}
      {disabled}
      aria-label={ariaLabel}
      {autofocus}
      bind:value
      oninput={(e) => oninput?.((e.currentTarget as HTMLInputElement).value)}
      {onkeydown}
      class="lw-text-field h-9 w-full rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] pr-3 pl-8 text-[13px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] disabled:opacity-45 {className}"
    />
  </div>
{:else}
  <!-- svelte-ignore a11y_autofocus -->
  <input
    {type}
    {placeholder}
    {disabled}
    aria-label={ariaLabel}
    {autofocus}
    bind:value
    oninput={(e) => oninput?.((e.currentTarget as HTMLInputElement).value)}
    {onkeydown}
    class="lw-text-field h-9 w-full rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3 text-[13px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] disabled:opacity-45 {className}"
  />
{/if}
