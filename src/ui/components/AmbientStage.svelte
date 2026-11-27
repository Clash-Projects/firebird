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
  let disabled = $derived(settingsStore.performanceMode || settingsStore.ambientStyle === 'off');
</script>

{#if resolved && !disabled}
  <div
    data-lw="ambient-stage"
    class="pointer-events-none absolute inset-0 overflow-hidden {className}"
    aria-hidden="true"
  >
    {#if settingsStore.ambientStyle === 'wash'}
      <div
        class="absolute inset-0 transition-[background] duration-1000"
        style:background="radial-gradient(ellipse 85% 70% at 35% 25%, {resolved} 0%, transparent 72%)"
        style:opacity={intensity}
      ></div>
    {:else}
      <!-- 4-region multi-tone mesh approximating WaveAmbientMesh in ambient.dart -->
      <div
        class="ambient-blob -top-28 -left-24 h-[520px] w-[660px]"
        style:background={resolved}
        style:opacity={intensity}
      ></div>
      <div
        class="ambient-blob top-[18%] right-[-14%] h-[460px] w-[540px]"
        style:background="color-mix(in srgb, {resolved} 68%, #4A6AC4)"
        style:opacity={intensity * 0.7}
      ></div>
      <div
        class="ambient-blob bottom-[-24%] left-[14%] h-[440px] w-[580px]"
        style:background="color-mix(in srgb, {resolved} 62%, #C45A88)"
        style:opacity={intensity * 0.55}
      ></div>
      <div
        class="ambient-blob right-[10%] bottom-[-18%] h-[360px] w-[420px]"
        style:background="color-mix(in srgb, {resolved} 75%, #1A1A1A)"
        style:opacity={intensity * 0.5}
      ></div>
    {/if}
  </div>
{/if}

<style>
  .ambient-blob {
    position: absolute;
    border-radius: 9999px;
    filter: blur(110px);
    transition: background-color 1400ms cubic-bezier(0.23, 1, 0.32, 1), opacity 700ms ease;
    will-change: background-color;
  }
</style>
