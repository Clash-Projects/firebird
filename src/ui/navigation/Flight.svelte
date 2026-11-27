<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    class?: string;
    children: Snippet;
  }

  let { class: className = '', children }: Props = $props();
</script>

<div class="lw-enter {className}" style:--lw-rise="8px" style:--lw-delay="0ms">
  {@render children()}
</div>
