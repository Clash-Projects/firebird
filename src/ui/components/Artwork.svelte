<script lang="ts">
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    src?: string;
    alt?: string;
    size?: number;
    radius?: number;
    circle?: boolean;
    shadow?: 'none' | 'lift' | 'artwork';
    class?: string;
  }

  let {
    src = '',
    alt = '',
    size,
    radius = 12,
    circle = false,
    shadow = 'none',
    class: className = '',
  }: Props = $props();

  let failed = $state(false);
  let shadowStyle = $derived(
    shadow === 'artwork'
      ? 'var(--lw-shadow-artwork)'
      : shadow === 'lift'
        ? 'var(--lw-shadow-lift)'
        : undefined
  );
</script>

<div
  data-lw="artwork"
  class="relative shrink-0 overflow-hidden bg-[var(--lw-surface-overlay)] {className}"
  style:width={size ? `${size}px` : undefined}
  style:height={size ? `${size}px` : undefined}
  style:border-radius={circle ? '999px' : `${radius}px`}
  style:box-shadow={shadowStyle}
>
  {#if src && !failed}
    <img
      {src}
      {alt}
      loading="lazy"
      decoding="async"
      onerror={() => (failed = true)}
      class="h-full w-full object-cover"
    />
  {:else}
    <div class="flex h-full w-full items-center justify-center text-[var(--lw-text-tertiary)]">
      <WaveIcon name="music" size={size ? Math.max(16, Math.round(size * 0.42)) : 28} />
    </div>
  {/if}
</div>
