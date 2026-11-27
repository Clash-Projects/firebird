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
    class?: string;
    onchange?: (value: T) => void;
  }

  let { value = $bindable(), options, class: className = '', onchange }: Props = $props();
</script>

<select
  bind:value
  onchange={(e) => onchange?.((e.currentTarget as HTMLSelectElement).value as T)}
  class="h-9 cursor-pointer rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3 text-[13px] text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] focus:outline-none {className}"
>
  {#each options as opt (opt.value)}
    <option value={opt.value}>{opt.label}</option>
  {/each}
</select>
