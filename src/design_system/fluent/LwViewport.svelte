<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    maxWidth?: number;
    class?: string;
    children: Snippet;
  }

  let { maxWidth = 1280, class: className = '', children }: Props = $props();
</script>

<div class="mx-auto w-full px-6 {className}" style:max-width="{maxWidth}px">
  {@render children()}
</div>
