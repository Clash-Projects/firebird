<script lang="ts">
  import type { AmLyrics } from '@uimaxbai/am-lyrics';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import { activeLineIndex, toTimedText, wordProgress } from './lyricAdapter';

  interface Props {
    compact?: boolean;
    onShareLine?: (text: string) => void;
  }

  let { compact = false, onShareLine }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);
  let posMs = $derived(Math.max(0, playerStore.positionSec * 1000 + offsetMs));
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs) : 0);
  let activeLine = $derived(doc?.lines[activeIdx]);
  let durationMs = $derived(playerStore.durationSec * 1000);
  let timedText = $derived(doc ? toTimedText(doc, durationMs, featuresStore.wordByWord) : null);
  let enhancedReady = $state(false);
  let lyricsView = $state<HTMLDivElement | null>(null);
  let lastManualScroll = -Infinity;

  $effect(() => {
    if (!timedText || enhancedReady) return;
    let mounted = true;
    void import('@uimaxbai/am-lyrics/am-lyrics.js').then(() => {
      if (mounted) enhancedReady = true;
    }).catch((error) => console.warn('Synced lyrics renderer failed to load:', error));
    return () => { mounted = false; };
  });

  function configureRenderer(element: AmLyrics) {
    const seek = (event: Event) => {
      const { timestamp } = (event as CustomEvent<{ timestamp: number }>).detail;
      if (Number.isFinite(timestamp)) seekLine(timestamp);
    };
    element.addEventListener('line-click', seek);

    // The pinned renderer exposes theme variables but no parts for its embedded controls.
    // LastWave supplies the controls and provider attribution around the scroll area.
    const chrome = document.createElement('style');
    // The footer supplies the renderer's end-of-list measurement, even when hidden.
    chrome.textContent = `
      :host .lyrics-header { display: none; }
      :host .lyrics-footer.lyrics-line {
        height: 0;
        min-height: 0;
        margin: 0;
        padding: 0;
        overflow: hidden;
        visibility: hidden;
      }
      :host(.sidebar-renderer) .lyrics-container {
        /* Reserve space so opening a translation keeps the final line visible. */
        padding-block: 22px calc(var(--lw-lyrics-translation-max) + var(--lw-lyrics-edge-fade));
        scrollbar-width: thin;
        scrollbar-color: var(--lw-scrollbar-thumb) transparent;
      }
      :host(.sidebar-renderer) .lyrics-container::-webkit-scrollbar {
        display: block;
        width: 6px;
      }
      :host(.sidebar-renderer) .lyrics-container::-webkit-scrollbar-thumb {
        border-radius: 8px;
        background: var(--lw-scrollbar-thumb);
      }
    `;
    void element.updateComplete.then(() => element.shadowRoot?.prepend(chrome));

    return { destroy: () => element.removeEventListener('line-click', seek) };
  }

  function shareActiveLine() {
    if (activeLine) onShareLine?.(activeLine.text);
  }

  function seekLine(timestamp: number) {
    playerStore.seek(Math.max(0, (timestamp - offsetMs) / 1000));
  }

  $effect(() => {
    if ((enhancedReady && timedText) || !doc?.synced || !lyricsView || !playerStore.isPlaying) return;
    const line = lyricsView.querySelector<HTMLElement>(`[data-line-index="${activeIdx}"]`);
    const container = lyricsView.closest<HTMLElement>('.lw-scroll');
    if (!line || !container || performance.now() - lastManualScroll < 3000) return;

    const lineRect = line.getBoundingClientRect();
    const centerOffset = lineRect.top - container.getBoundingClientRect().top + (lineRect.height - container.clientHeight) / 2;
    const reduced = settingsStore.reduceMotion || matchMedia('(prefers-reduced-motion: reduce)').matches;
    container.scrollTo({ top: container.scrollTop + centerOffset, behavior: reduced ? 'instant' : 'smooth' });
  });
</script>

{#if enhancedReady && timedText && doc}
  <div class="synced-lyrics" class:compact data-lw="lyrics-view">
    {#key doc.trackId}
      <am-lyrics
        use:configureRenderer
        class:sidebar-renderer={compact}
        ttml={timedText}
        currentTime={posMs}
        duration={durationMs || undefined}
        paused={!playerStore.isPlaying || playerStore.isBuffering}
        interpolate={playerStore.speed === 1}
        noAutoAlternates={true}
        noBlur={compact || settingsStore.reduceMotion}
        reducedMotion={settingsStore.reduceMotion}
        fontFamily="var(--lw-font-family)"
        style:--lw-lyrics-scale={featuresStore.lyricsFontScale}
      ></am-lyrics>
    {/key}
    <footer class="lyrics-footer">
      {#if (featuresStore.showTranslation && activeLine?.translation) || (featuresStore.showRomanized && activeLine?.romanized)}
        <div class="active-translation lw-scroll">
          {#if featuresStore.showRomanized && activeLine?.romanized}<div>{activeLine.romanized}</div>{/if}
          {#if featuresStore.showTranslation && activeLine?.translation}<div>{activeLine.translation}</div>{/if}
        </div>
      {/if}
      <div class="lyrics-meta">
        <span title={doc.provider}>Source: {doc.provider}</span>
        {#if onShareLine && activeLine?.text}
          <button type="button" onclick={shareActiveLine} title="Share lyric card">Share line</button>
        {/if}
      </div>
    </footer>
  </div>
{:else}
<div bind:this={lyricsView} onwheel={() => (lastManualScroll = performance.now())} data-lw="lyrics-view" class="lyrics-fallback mx-auto w-full max-w-[660px] py-4" class:compact class:lw-scroll={compact} style:--lw-lyrics-scale={featuresStore.lyricsFontScale}>
  {#if doc}
    {#each doc.lines as line, i}
      {@const active = doc.synced && i === activeIdx}
      {@const past = doc.synced && i < activeIdx}
      <div
        data-lw="lyrics-line"
        data-line-index={i}
        data-active={active}
        class="lyric-line group"
        class:past
      >
        <button
          type="button"
          onclick={() => doc.synced && seekLine(line.startMs)}
          class="min-w-0 flex-1 cursor-pointer text-left"
          aria-current={active ? 'true' : undefined}
        >
          <div class="lyric-text">
            {#if line.words && active && featuresStore.wordByWord}
              {#each line.words as w, wi (w.startMs)}
                {@const p = wordProgress(w.startMs, w.endMs, posMs)}
                <span
                  class="inline-block transition-opacity duration-[var(--lw-motion-fast)]"
                  style:opacity={0.8 + p * 0.2}
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
    <div class="px-3 pt-4 text-[11px] text-[var(--lw-text-tertiary)]">
      Source: {doc.provider}
    </div>
  {:else}
    <div class="px-3 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      {featuresStore.lyricsLoading ? 'Fetching synced lyrics...' : track ? 'No lyrics available for this track.' : 'Play a track to view lyrics.'}
    </div>
  {/if}
</div>
{/if}

<style>
  .synced-lyrics {
    display: flex;
    width: min(100%, 660px);
    height: 100%;
    min-height: 0;
    flex-direction: column;
    margin-inline: auto;
  }
  am-lyrics {
    min-height: 0;
    flex: 1 1 0;
    --am-lyrics-highlight-color: var(--lw-text-primary);
    --am-lyrics-highlight-surface: color-mix(in srgb, var(--lw-accent) 12%, transparent);
    --am-lyrics-highlight-radius: 10px;
    --am-lyrics-inline-padding: 4px;
    --am-lyrics-compact-font-size: calc(23px * var(--lw-lyrics-scale));
    --am-lyrics-wide-font-size: calc(32px * var(--lw-lyrics-scale));
    --am-lyrics-compact-line-spacing: 13px;
    --am-lyrics-wide-line-spacing: 16px;
  }
  .compact {
    --lw-sidebar-lyric-size: 24px;
    --lw-sidebar-lyric-spacing: 22px;
    --lw-lyrics-translation-max: min(96px, 14dvh);
    --lw-lyrics-edge-fade: 22px;
  }
  .compact am-lyrics {
    mask-image: linear-gradient(to bottom, transparent, black 10px, black calc(100% - var(--lw-lyrics-edge-fade)), transparent);
    --am-lyrics-compact-font-size: calc(var(--lw-sidebar-lyric-size) * var(--lw-lyrics-scale));
    --am-lyrics-wide-font-size: var(--am-lyrics-compact-font-size);
    --am-lyrics-compact-line-spacing: var(--lw-sidebar-lyric-spacing);
    --am-lyrics-wide-line-spacing: var(--lw-sidebar-lyric-spacing);
  }
  .lyrics-fallback.compact { min-height: 0; flex: 1; }
  .lyrics-footer { flex: none; }
  .compact .lyrics-footer { border-top: 1px solid var(--lw-divider); }
  .lyrics-meta {
    display: flex;
    min-height: 34px;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 8px;
    color: var(--lw-text-secondary);
    font-size: 11px;
  }
  .lyrics-meta span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lyrics-meta button {
    flex: none;
    padding: 4px 8px;
    border: 1px solid var(--lw-outline-soft);
    border-radius: 7px;
    color: var(--lw-text-primary);
    cursor: pointer;
  }
  .lyrics-meta button:hover { background: var(--lw-wash-hover); }
  .active-translation {
    margin: 0;
    padding: 8px;
    border-top: 1px solid var(--lw-divider);
    color: var(--lw-text-secondary);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .compact .active-translation { max-block-size: var(--lw-lyrics-translation-max); border-top: 0; }
  .lyric-line {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    color: var(--lw-text-secondary);
    transition: background-color var(--lw-motion-normal), color var(--lw-motion-normal);
  }
  .compact .lyric-line { padding-block: calc(var(--lw-sidebar-lyric-spacing) / 2); }
  .lyric-line.past { color: var(--lw-text-tertiary); }
  .lyric-line[data-active="true"] { background: color-mix(in srgb, var(--lw-accent) 10%, transparent); color: var(--lw-text-primary); }
  .lyric-line::before {
    position: absolute;
    inset: 10px auto 10px 0;
    width: 2px;
    border-radius: 3px;
    background: var(--lw-accent);
    opacity: 0;
    content: '';
    transition: opacity var(--lw-motion-normal);
  }
  .lyric-line[data-active="true"]::before { opacity: 1; }
  .lyric-text { font-size: calc(23px * var(--lw-lyrics-scale)); font-weight: 650; line-height: 1.36; overflow-wrap: anywhere; }
  .compact .lyric-text { font-size: calc(var(--lw-sidebar-lyric-size) * var(--lw-lyrics-scale)); line-height: 1.22; }
  @media (hover: hover) and (pointer: fine) {
    .lyric-line[data-active="false"]:hover { background: var(--lw-wash-hover); color: var(--lw-text-primary); }
  }
  @media (forced-colors: active) {
    .compact am-lyrics { mask-image: none; }
  }
</style>
