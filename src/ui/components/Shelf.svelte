<script lang="ts">
  import type { Snippet } from 'svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    title: string;
    subtitle?: string;
    actionLabel?: string;
    onAction?: () => void;
    children: Snippet;
  }

  let { title, subtitle, actionLabel, onAction, children }: Props = $props();
</script>

<section data-lw="shelf" class="space-y-3">
  <div class="flex items-end justify-between gap-3">
    <div>
      <h2 class="text-[15px] font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]">{title}</h2>
      {#if subtitle}
        <p class="text-[12px] text-[var(--lw-text-secondary)]">{subtitle}</p>
      {/if}
    </div>
    {#if actionLabel && onAction}
      <button
        type="button"
        onclick={onAction}
        class="inline-flex cursor-pointer items-center gap-1 rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        {actionLabel}
        <WaveIcon name="chevronRight" size={14} />
      </button>
    {/if}
  </div>
  {@render children()}
</section>
