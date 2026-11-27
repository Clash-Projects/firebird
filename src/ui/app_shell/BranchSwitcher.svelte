<script lang="ts">
  import type { Snippet } from 'svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';

  interface Props {
    routeKey: string;
    hasDockSpacer?: boolean;
    fillHeight?: boolean;
    children: Snippet;
  }

  let { routeKey, hasDockSpacer = false, fillHeight = false, children }: Props = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  const savedScroll = new Map<string, number>();
  let lastKey = '';

  $effect(() => {
    const el = scroller;
    const key = routeKey;
    if (!el) return;
    if (lastKey && lastKey !== key) {
      savedScroll.set(lastKey, el.scrollTop);
    }
    lastKey = key;
    const restoreY = savedScroll.get(key) ?? 0;
    requestAnimationFrame(() => {
      if (el) el.scrollTop = restoreY;
    });
  });

  function onScroll() {
    if (scroller && routeKey) {
      savedScroll.set(routeKey, scroller.scrollTop);
    }
  }
</script>

<div
  bind:this={scroller}
  onscroll={onScroll}
  use:waveSmoothScroll={{ axis: 'y' }}
  class="lw-scroll stage-scroll relative z-10 h-full w-full"
>
  {#key routeKey}
    <div
      class="lw-route-enter w-full {fillHeight ? 'h-full overflow-hidden' : 'min-h-full'}"
      style:padding-bottom={hasDockSpacer ? 'var(--lw-dock-h, 68px)' : undefined}
    >
      {@render children()}
    </div>
  {/key}
</div>

<style>
  .stage-scroll {
    overscroll-behavior: contain;
  }
</style>
