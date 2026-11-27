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

<!-- Editorial calm empty state matching WaveEmpty in states.dart (no heavy outer card border) -->
<div class="mx-auto flex max-w-[420px] flex-col items-center justify-center px-7 py-14 text-center">
  <div
    class="mb-3 flex h-16 w-16 items-center justify-center rounded-[12px] border border-[var(--lw-outline-soft)] text-[var(--lw-text-secondary)]"
    style:background="linear-gradient(135deg, var(--lw-wash-hover), transparent)"
  >
    <WaveIcon name={icon} size={24} />
  </div>
  <h3 class="text-[16px] font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]">{title}</h3>
  {#if description}
    <p class="mt-1 text-[12px] leading-[1.45] text-[var(--lw-text-secondary)]">{description}</p>
  {/if}
  {#if action}
    <div class="mt-3">
      {@render action()}
    </div>
  {/if}
</div>
