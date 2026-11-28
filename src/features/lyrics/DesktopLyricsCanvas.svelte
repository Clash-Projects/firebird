<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import { activeLineIndex } from './lyricAdapter';

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);
  let posMs = $derived(playerStore.positionSec * 1000 - offsetMs);
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs, doc.synced) : -1);
  let activeLine = $derived(doc && activeIdx >= 0 ? (doc.lines[activeIdx] ?? null) : null);
</script>

{#if activeLine}
  <WaveHaze
    level="l3"
    class="rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-panel-translucent)] px-5 py-3 text-center shadow-[var(--lw-shadow-flyout)]"
  >
    <div class="text-[18px] font-bold text-[var(--lw-text-primary)]">{activeLine.text}</div>
    {#if featuresStore.showRomanized && activeLine.romanized}
      <div class="mt-0.5 text-[12.5px] italic text-[var(--lw-text-secondary)]">{activeLine.romanized}</div>
    {/if}
    {#if featuresStore.showTranslation && activeLine.translation}
      <div class="mt-0.5 text-[13px] text-[var(--lw-text-secondary)]">{activeLine.translation}</div>
    {/if}
  </WaveHaze>
{/if}
