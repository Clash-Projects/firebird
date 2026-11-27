<script lang="ts">
  import type { Snippet } from 'svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';

  interface Props {
    class?: string;
    axis?: 'y' | 'x';
    children: Snippet;
  }

  let { class: className = '', axis = 'y', children }: Props = $props();
</script>

<div
  class="{axis === 'x' ? 'lw-scroll-x' : 'lw-scroll'} {className}"
  use:waveSmoothScroll={{ axis, drag: axis === 'x' }}
>
  {@render children()}
</div>
