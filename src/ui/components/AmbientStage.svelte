<script lang="ts">
  import { cubicOut } from 'svelte/easing';
  import { prefersReducedMotion } from 'svelte/motion';
  import { fade } from 'svelte/transition';
  import { settingsStore } from '$src/state/settings.svelte';

  interface Props {
    artworkUrl: string;
    intensity?: number;
    class?: string;
  }

  let { artworkUrl, intensity = 0.28, class: className = '' }: Props = $props();
  let disabled = $derived(settingsStore.performanceMode || settingsStore.ambientStyle === 'off');
  let reducedMotion = $derived(settingsStore.reduceMotion || prefersReducedMotion.current);
</script>

{#if artworkUrl && !disabled}
  <div
    data-lw="ambient-stage"
    class="pointer-events-none absolute inset-0 overflow-hidden {className}"
    aria-hidden="true"
    style:--ambient-intensity={intensity}
  >
    {#key artworkUrl}
      <div
        class="ambient-art absolute inset-0"
        class:wash={settingsStore.ambientStyle === 'wash'}
        transition:fade={{ duration: reducedMotion ? 0 : 480, easing: cubicOut }}
      >
        <img
          src={artworkUrl}
          alt=""
          decoding="async"
          class="ambient-image"
          onerror={(event) => event.currentTarget.setAttribute('hidden', '')}
        />
      </div>
    {/key}
  </div>
{/if}

<style>
  .ambient-art {
    opacity: var(--ambient-intensity);
    mask-image:
      radial-gradient(ellipse 80% 80% at 18% 24%, black 8%, transparent 90%),
      radial-gradient(ellipse 65% 70% at 86% 70%, rgb(0 0 0 / 62%) 8%, transparent 86%);
  }

  .ambient-art.wash {
    mask-image: radial-gradient(ellipse 85% 75% at 35% 25%, black, transparent 78%);
  }

  .ambient-image {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 25%;
    height: 25%;
    max-width: none;
    /* Blur a smaller layer before scaling it into the backdrop. */
    filter: blur(24px) brightness(0.84);
    transform: translate(-50%, -50%) scale(4.8);
  }

  :global(:root[data-theme='light']) .ambient-art {
    opacity: calc(var(--ambient-intensity) * 0.55);
  }
</style>
