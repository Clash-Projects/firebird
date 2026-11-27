<script lang="ts">
  import { playerStore } from '$src/state/player.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { activeLineIndex } from './lyricAdapter';

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let posMs = $derived(playerStore.positionSec * 1000);
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs) : 0);
  let activeLine = $derived(doc?.lines[activeIdx] ?? null);
</script>

{#if activeLine}
  <div class="rounded-[16px] border border-[var(--lw-outline)] bg-black/70 px-5 py-3 text-center backdrop-blur-md">
    <div class="text-[18px] font-bold text-white">{activeLine.text}</div>
    {#if featuresStore.showTranslation && activeLine.translation}
      <div class="mt-0.5 text-[13px] text-white/70">{activeLine.translation}</div>
    {/if}
  </div>
{/if}
