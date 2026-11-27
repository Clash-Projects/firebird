<script lang="ts">
  import type { Snippet } from 'svelte';
  import { resolveHazeBlur, type LwHazeLevel } from './haze.svelte';

  interface Props {
    level?: LwHazeLevel;
    enableBlur?: boolean;
    class?: string;
    dataLw?: string;
    children: Snippet;
  }

  let {
    level = 'l1',
    enableBlur = true,
    class: className = '',
    dataLw,
    children,
  }: Props = $props();

  let blurPx = $derived(resolveHazeBlur(level, enableBlur));
</script>

<div
  data-lw={dataLw}
  class={className}
  style:backdrop-filter={blurPx > 0 ? `blur(${blurPx}px)` : undefined}
  style:-webkit-backdrop-filter={blurPx > 0 ? `blur(${blurPx}px)` : undefined}
>
  {@render children()}
</div>
