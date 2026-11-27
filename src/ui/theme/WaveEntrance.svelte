<script lang="ts">
  import type { Snippet } from 'svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import { entranceDelayMs, WAVE_ENTRANCE_RISE_PX } from './motion';

  interface Props {
    index?: number;
    rise?: number;
    class?: string;
    children: Snippet;
  }

  let { index = 0, rise = WAVE_ENTRANCE_RISE_PX, class: className = '', children }: Props = $props();

  let delay = $derived(entranceDelayMs(index));
</script>

<div
  class="{settingsStore.reduceMotion ? '' : 'lw-enter'} {className}"
  style:--lw-delay="{delay}ms"
  style:--lw-rise="{rise}px"
>
  {@render children()}
</div>
