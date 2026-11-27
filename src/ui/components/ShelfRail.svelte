<script lang="ts">
  import type { Snippet } from 'svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let { title, eyebrow, onSeeAll, children }: {
    title: string;
    eyebrow?: string;
    onSeeAll: () => void;
    children: Snippet;
  } = $props();

  const id = $props.id();
  let viewport = $state<HTMLDivElement>();
  let track = $state<HTMLDivElement>();
  let canGoBack = $state(false);
  let canGoForward = $state(false);

  function updateEdges() {
    if (!viewport) return;
    const offset = Math.abs(viewport.scrollLeft);
    canGoBack = offset > 2;
    canGoForward = offset < viewport.scrollWidth - viewport.clientWidth - 2;
  }

  function move(direction: -1 | 1) {
    if (!viewport) return;
    const axis = getComputedStyle(viewport).direction === 'rtl' ? -1 : 1;
    viewport.scrollBy({
      left: direction * axis * viewport.clientWidth * 0.78,
      behavior: 'smooth',
    });
  }

  $effect(() => {
    if (!viewport || !track) return;
    updateEdges();
    const observer = new ResizeObserver(updateEdges);
    observer.observe(viewport);
    observer.observe(track);
    return () => observer.disconnect();
  });
</script>

<section aria-labelledby={`${id}-title`} class="min-w-0 space-y-2.5">
  <div class="flex items-end justify-between gap-3">
    <div class="min-w-0">
      {#if eyebrow}
        <p class="mb-0.5 text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">{eyebrow}</p>
      {/if}
      <h2 id={`${id}-title`} class="truncate text-[15px] font-semibold tracking-[-0.1px] leading-[1.3] text-[var(--lw-text-primary)]">
        {title}
      </h2>
    </div>
    <div class="flex shrink-0 items-center gap-1">
      <button type="button" class="lw-press shelf-all" onclick={onSeeAll}>See all</button>
      {#if canGoBack || canGoForward}
        <button
          type="button"
          class="lw-press shelf-arrow"
          use:waveTooltip={`Scroll ${title.toLowerCase()} left`}
          aria-label={`Previous ${title.toLowerCase()}`}
          aria-controls={`${id}-rail`}
          aria-disabled={!canGoBack}
          onclick={() => canGoBack && move(-1)}
        >
          <WaveIcon name="chevronLeft" size={14} />
        </button>
        <button
          type="button"
          class="lw-press shelf-arrow"
          use:waveTooltip={`Scroll ${title.toLowerCase()} right`}
          aria-label={`More ${title.toLowerCase()}`}
          aria-controls={`${id}-rail`}
          aria-disabled={!canGoForward}
          onclick={() => canGoForward && move(1)}
        >
          <WaveIcon name="chevronRight" size={14} />
        </button>
      {/if}
    </div>
  </div>
  <div
    id={`${id}-rail`}
    bind:this={viewport}
    use:waveSmoothScroll={{ axis: 'x', drag: true }}
    class="lw-scroll-x shelf-viewport"
    onscroll={updateEdges}
  >
    <div bind:this={track} class="shelf-track">{@render children()}</div>
  </div>
</section>

<style>
  .shelf-viewport {
    scroll-snap-type: inline proximity;
    scroll-padding-inline: 4px;
    cursor: grab;
  }
  .shelf-viewport:active { cursor: grabbing; }
  .shelf-track {
    display: flex;
    inline-size: max-content;
    min-inline-size: 100%;
    gap: 12px;
    padding: 5px 4px 8px;
  }
  .shelf-track > :global(*) { scroll-snap-align: start; }
  .shelf-arrow, .shelf-all {
    display: grid;
    height: 28px;
    place-items: center;
    border-radius: 6px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: background-color var(--lw-motion-fast) var(--ease-native), color var(--lw-motion-fast) var(--ease-native);
  }
  .shelf-arrow {
    width: 28px;
    border: 1px solid var(--lw-outline-soft);
    background: var(--lw-surface);
  }
  .shelf-all {
    padding-inline: 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--lw-accent);
  }
  .shelf-arrow:dir(rtl) :global(svg) { transform: scaleX(-1); }
  .shelf-arrow[aria-disabled="true"] { opacity: 0.35; cursor: default; }
  .shelf-arrow[aria-disabled="true"]:active { scale: 1; }
  @media (hover: hover) {
    .shelf-arrow:not([aria-disabled="true"]):hover {
      background: var(--lw-wash-hover);
      color: var(--lw-text-primary);
    }
    .shelf-all:hover {
      text-decoration: underline;
    }
  }
</style>
