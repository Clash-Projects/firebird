<script lang="ts">
  import type { Snippet } from 'svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import type { LwHazeLevel } from '$src/ui/theme/haze.svelte';

  interface Props {
    level?: LwHazeLevel;
    enableBlur?: boolean;
    class?: string;
    children: Snippet;
  }

  let { level = 'l1', enableBlur = true, class: className = '', children }: Props = $props();
</script>

<WaveHaze {level} {enableBlur} class={className}>
  {@render children()}
</WaveHaze>
