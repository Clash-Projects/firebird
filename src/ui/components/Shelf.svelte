<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    overline?: string;
    count?: number;
    subtitle?: string;
    actionLabel?: string;
    onAction?: () => void;
    children: Snippet;
  }

  let { title, overline, count, subtitle, actionLabel, onAction, children }: Props = $props();
</script>

<section data-lw="shelf" class="space-y-2.5">
  <div class="flex items-end justify-between gap-3">
    <div class="min-w-0">
      {#if overline}
        <p class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          {overline}
        </p>
      {/if}
      <div class="flex items-center gap-1.5">
        <h2 class="truncate text-[15px] font-semibold tracking-[-0.1px] leading-[1.3] text-[var(--lw-text-primary)]">
          {title}
        </h2>
        {#if count !== undefined}
          <span class="text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{count}</span>
        {/if}
      </div>
      {#if subtitle}
        <p class="mt-0.5 text-[12px] text-[var(--lw-text-secondary)]">{subtitle}</p>
      {/if}
    </div>
    {#if actionLabel && onAction}
      <button
        type="button"
        onclick={onAction}
        class="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-[6px] px-1.5 py-0.5 text-[12px] font-medium text-[var(--lw-accent)] transition-colors duration-[120ms] hover:underline"
      >
        {actionLabel}
      </button>
    {/if}
  </div>
  {@render children()}
</section>
