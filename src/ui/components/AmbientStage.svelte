<script lang="ts">
  import { activeAmbientColor } from './ambient.svelte';
  import { settingsStore } from '$src/state/settings.svelte';

  interface Props {
    color?: string | null;
    intensity?: number;
    class?: string;
  }

  let { color, intensity = 0.28, class: className = '' }: Props = $props();
  let resolved = $derived(color ?? activeAmbientColor());
</script>

{#if resolved && settingsStore.ambientStyle !== 'off'}
  <div
    data-lw="ambient-stage"
    class="pointer-events-none absolute inset-0 overflow-hidden {className}"
    aria-hidden="true"
  >
    <div
      class="absolute -top-24 -left-24 h-[480px] w-[620px] rounded-full blur-[90px] transition-[background-color] duration-700"
      style:background={resolved}
      style:opacity={intensity}
    ></div>
    {#if settingsStore.ambientStyle === 'aurora' || settingsStore.ambientStyle === 'mesh'}
      <div
        class="absolute top-1/3 right-[-10%] h-[420px] w-[520px] rounded-full blur-[110px] transition-[background-color] duration-700"
        style:background={resolved}
        style:opacity={intensity * 0.65}
      ></div>
    {/if}
  </div>
{/if}
