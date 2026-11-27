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
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  let smoothPosMs = $state(0);
  let lineEls = $state<Record<number, HTMLDivElement | null>>({});
  let lastScrolledIdx = -1;

  $effect(() => {
    const baseMs = playerStore.positionSec * 1000;
    const playing = playerStore.isPlaying;
    const speed = playerStore.speed || 1;
    const anchor = performance.now();
    smoothPosMs = baseMs;

    if (!playing) return;

    let rafId = 0;
    const tick = (now: number) => {
      smoothPosMs = baseMs + (now - anchor) * speed;
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  });

  let effectivePosMs = $derived(Math.max(0, smoothPosMs + offsetMs));
  let activeIdx = $derived(doc?.synced ? activeLineIndex(doc.lines, effectivePosMs) : -1);

  $effect(() => {
    if (!doc?.synced || activeIdx < 0 || activeIdx === lastScrolledIdx) return;
    lastScrolledIdx = activeIdx;
    const el = lineEls[activeIdx];
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  });
</script>

<div data-lw="lyrics-view" class="mx-auto w-full max-w-[620px] space-y-3 py-6">
  {#if featuresStore.lyricsLoading && !doc}
    <div class="px-3 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      Fetching synced lyrics...
    </div>
  {:else if doc}
    {#each doc.lines as line, i (`${i}-${line.startMs}`)}
      {@const active = doc.synced && i === activeIdx}
      {@const past = doc.synced && i < activeIdx}
      {@const distance = activeIdx >= 0 ? Math.abs(i - activeIdx) : 0}
      <div
        bind:this={lineEls[i]}
        data-lw="lyrics-line"
        data-active={active}
        class="group flex items-start gap-2 rounded-[14px] px-3 py-2 transition-all duration-300 ease-out hover:bg-[var(--lw-wash-hover)]"
        style:transform={active ? 'scale(1.025)' : 'scale(1)'}
        style:transform-origin="left center"
        style:filter={!active && doc.synced && distance >= 2 ? 'blur(0.6px)' : 'none'}
      >
        <button
          type="button"
          onclick={() => doc.synced && playerStore.seek(Math.max(0, (line.startMs - offsetMs) / 1000))}
          class="flex-1 cursor-pointer text-left"
        >
          <div
            class="leading-[1.35] transition-all duration-300 {active
              ? 'font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]'
              : past
                ? 'font-medium text-[var(--lw-text-tertiary)] opacity-55 group-hover:opacity-90'
                : 'font-medium text-[var(--lw-text-secondary)] opacity-70 group-hover:opacity-95'}"
            style:font-size="{Math.round((active ? (compact ? 19 : 24) : (compact ? 15 : 17)) * featuresStore.lyricsFontScale)}px"
          >
            {#if line.words && line.words.length > 0 && active && featuresStore.wordByWord}
              {#each line.words as w, wi (`${wi}-${w.startMs}`)}
                {@const p = wordProgress(w.startMs, w.endMs, effectivePosMs)}
                {@const pct = Math.round(p * 100)}
                <span
                  class="inline-block will-change-transform"
                  style:transform="translateY({p > 0 && p < 1 ? -1.5 : 0}px)"
                  style:transition="transform 140ms ease-out"
                  style:background-image="linear-gradient(90deg, var(--lw-text-primary) 0%, var(--lw-text-primary) {pct}%, color-mix(in srgb, var(--lw-text-primary) 32%, transparent) {pct}%)"
                  style:-webkit-background-clip="text"
                  style:background-clip="text"
                  style:color="transparent"
                >{w.text}{wi < line.words.length - 1 ? '\u00A0' : ''}</span>
              {/each}
            {:else}
              {line.text}
            {/if}
          </div>

          {#if featuresStore.showRomanized && line.romanized}
            <div class="mt-1 text-[12.5px] text-[var(--lw-text-tertiary)]">{line.romanized}</div>
          {/if}

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
    <div class="flex items-center justify-between px-3 pt-4 text-[11px] text-[var(--lw-text-tertiary)]">
      <span>Source: {doc.provider}{doc.wordSynced ? ' · Word-Synced Karaoke' : doc.synced ? ' · Line-Synced' : ' · Plain'}</span>
    </div>
  {:else}
    <div class="px-3 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      {track ? 'No lyrics available for this track.' : 'Play a track to view lyrics.'}
    </div>
  {/if}
</div>
