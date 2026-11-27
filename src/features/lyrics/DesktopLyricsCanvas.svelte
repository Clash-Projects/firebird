<script lang="ts">
  import { playerStore } from '$src/state/player.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import { activeLineIndex } from './lyricAdapter';

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let posMs = $derived(playerStore.positionSec * 1000);
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs) : 0);
  let activeLine = $derived(doc?.lines[activeIdx] ?? null);
</script>

{#if activeLine}
  <WaveHaze
    level="l3"
    class="rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-panel-translucent)] px-5 py-3 text-center shadow-[var(--lw-shadow-flyout)]"
  >
    <div class="text-[18px] font-bold text-[var(--lw-text-primary)]">{activeLine.text}</div>
    {#if featuresStore.showTranslation && activeLine.translation}
      <div class="mt-0.5 text-[13px] text-[var(--lw-text-secondary)]">{activeLine.translation}</div>
    {/if}
  </WaveHaze>
{/if}
