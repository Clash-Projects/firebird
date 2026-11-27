<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { activeLineIndex, wordProgress } from './lyricAdapter';

  interface Props {
    compact?: boolean;
    onShareLine?: (text: string) => void;
  }

  let { compact = false, onShareLine }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let posMs = $derived(playerStore.positionSec * 1000);
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs) : 0);
</script>

<div data-lw="lyrics-view" class="mx-auto w-full max-w-[620px] space-y-4 py-4">
  {#if doc}
    {#each doc.lines as line, i (line.startMs)}
      {@const active = i === activeIdx}
      {@const past = i < activeIdx}
      <div
        data-lw="lyrics-line"
        data-active={active}
        class="group flex items-start gap-2 rounded-[12px] px-3 py-1.5 transition-all duration-[180ms] hover:bg-[var(--lw-wash-hover)]"
      >
        <button
          type="button"
          onclick={() => playerStore.seek(line.startMs / 1000)}
          class="flex-1 cursor-pointer text-left"
        >
          <div
            class="transition-all duration-[180ms] {active
              ? compact
                ? 'text-[18px] font-bold text-[var(--lw-text-primary)]'
                : 'text-[22px] font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]'
              : past
                ? 'text-[16px] font-medium text-[var(--lw-text-tertiary)]'
                : 'text-[16px] font-medium text-[var(--lw-text-secondary)]'}"
            style:font-size="{Math.round((active ? (compact ? 18 : 22) : 16) * featuresStore.lyricsFontScale)}px"
          >
            {#if line.words && active}
              {#each line.words as w, wi (w.startMs)}
                {@const p = wordProgress(w.startMs, w.endMs, posMs)}
                <span
                  class="inline-block transition-opacity duration-100"
                  style:opacity={0.42 + p * 0.58}
                >{w.text}{wi < line.words.length - 1 ? '\u00A0' : ''}</span>
              {/each}
            {:else}
              {line.text}
            {/if}
          </div>

          {#if featuresStore.showTranslation && line.translation}
            <div class="mt-1 text-[13px] text-[var(--lw-text-secondary)]">{line.translation}</div>
          {/if}
        </button>

        {#if onShareLine}
          <button
            type="button"
            onclick={() => onShareLine(line.text)}
            title="Share lyric card"
            class="mt-1 shrink-0 cursor-pointer rounded-[6px] px-2 py-1 text-[11px] text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 hover:bg-[var(--lw-wash-pressed)] hover:text-[var(--lw-text-primary)]"
          >
            Share
          </button>
        {/if}
      </div>
    {/each}
    <div class="px-3 pt-4 text-[11px] text-[var(--lw-text-tertiary)]">
      Source: {doc.provider}
    </div>
  {:else}
    <div class="px-3 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      {track ? 'No lyrics available for this track.' : 'Play a track to view lyrics.'}
    </div>
  {/if}
</div>
