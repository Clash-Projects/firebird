<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="relative flex h-full w-full flex-col overflow-hidden">
  {@render children()}
</div>
