<script lang="ts">
  interface Props {
    value: string;
    placeholder?: string;
    type?: string;
    class?: string;
    autofocus?: boolean;
    oninput?: (value: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    class: className = '',
    autofocus = false,
    oninput,
    onkeydown,
  }: Props = $props();
</script>

<!-- svelte-ignore a11y_autofocus -->
<input
  {type}
  {placeholder}
  {autofocus}
  bind:value
  oninput={(e) => oninput?.((e.currentTarget as HTMLInputElement).value)}
  {onkeydown}
  class="h-9 w-full rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-3 text-[13px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] focus:border-[var(--lw-accent)] focus:outline-none {className}"
/>
