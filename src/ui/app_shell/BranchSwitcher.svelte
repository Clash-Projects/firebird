<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    routeKey: string;
    children: Snippet;
  }

  let { routeKey, children }: Props = $props();
</script>

{#key routeKey}
  <div class="lw-scroll stage-scroll relative z-10 h-full w-full scrollbar-gutter-stable">
    <div class="lw-route-enter h-full w-full">
      {@render children()}
    </div>
  </div>
{/key}

<style>
  .stage-scroll { overscroll-behavior: contain; scroll-behavior: var(--lw-scroll-behavior); }
</style>
