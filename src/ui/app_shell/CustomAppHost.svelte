<script lang="ts">
  import type { CustomAppRouteSlot, OverrideViewSlot } from '$src/lib/plugins/types';

  interface Props {
    app: CustomAppRouteSlot | OverrideViewSlot;
  }

  let { app }: Props = $props();
  let hostEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!hostEl) return;
    hostEl.innerHTML = '';
    let cleanup: void | (() => void);
    try {
      cleanup = app.mount(hostEl);
    } catch (err) {
      console.error('[LastWave] Plugin view mount error:', err);
    }
    return () => {
      try {
        if (typeof cleanup === 'function') cleanup();
      } catch {
        // Ignore plugin view teardown errors.
      }
    };
  });
</script>

<div data-lw="custom-app-view" bind:this={hostEl} class="h-full w-full"></div>
