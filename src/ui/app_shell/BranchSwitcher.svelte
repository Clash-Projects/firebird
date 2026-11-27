<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    routeKey: string;
    children: Snippet;
  }

  let { routeKey, children }: Props = $props();
</script>

{#key routeKey}
  <div class="lw-enter h-full w-full" style:--lw-rise="8px" style:--lw-delay="0ms">
    {@render children()}
  </div>
{/key}
