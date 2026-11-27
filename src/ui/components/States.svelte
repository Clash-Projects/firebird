<script lang="ts">
  import type { Snippet } from 'svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    icon?: WaveIconName;
    title: string;
    description?: string;
    action?: Snippet;
  }

  let { icon = 'music', title, description, action }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center rounded-[20px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-6 py-14 text-center">
  <div class="mb-3 flex h-12 w-12 items-center justify-center rounded-[14px] bg-[var(--lw-surface-overlay)] text-[var(--lw-text-secondary)]">
    <WaveIcon name={icon} size={22} />
  </div>
  <h3 class="text-[15px] font-semibold text-[var(--lw-text-primary)]">{title}</h3>
  {#if description}
    <p class="mt-1 max-w-[380px] text-[12.5px] text-[var(--lw-text-secondary)]">{description}</p>
  {/if}
  {#if action}
    <div class="mt-4">
      {@render action()}
    </div>
  {/if}
</div>
