<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    subtitle?: string;
    actions?: Snippet;
    class?: string;
  }

  let { title, subtitle, actions, class: className = '' }: Props = $props();
</script>

<div class="flex flex-wrap items-center justify-between gap-3 {className}">
  <div class="min-w-0">
    {#if title}
      <h1 class="truncate text-[22px] font-bold tracking-[-0.4px] leading-[1.15]">{title}</h1>
    {/if}
    {#if subtitle}
      <p class="mt-0.5 text-[12.5px] text-[var(--lw-text-secondary)]">{subtitle}</p>
    {/if}
  </div>
  {#if actions}
    <div class="flex items-center gap-2">
      {@render actions()}
    </div>
  {/if}
</div>
