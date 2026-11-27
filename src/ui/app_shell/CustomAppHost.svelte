<script lang="ts">
  import type { CustomAppRouteSlot } from '$src/lib/plugins/types';

  interface Props {
    app: CustomAppRouteSlot;
  }

  let { app }: Props = $props();
  let hostEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!hostEl) return;
    hostEl.innerHTML = '';
    const cleanup = app.mount(hostEl);
    return () => {
      if (typeof cleanup === 'function') cleanup();
    };
  });
</script>

<div data-lw="custom-app-view" bind:this={hostEl} class="h-full w-full"></div>
