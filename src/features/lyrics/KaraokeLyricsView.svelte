<script lang="ts">
  import type { AmLyrics } from '@uimaxbai/am-lyrics';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { activeLineIndex, keepScrollOnSungWord, toTimedText, wordProgress } from './lyricAdapter';

  interface Props {
    compact?: boolean;
    fontSize?: number;
    showHeaderControls?: boolean;
    onShareLine?: (text: string) => void;
  }

  let { compact = false, fontSize, showHeaderControls = true, onShareLine }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  // 60fps interpolated audio clock (mirrors OG Flutter WaveKaraokeLyricsView Ticker)
  let baseAudioMs = $state(playerStore.positionSec * 1000);
  let baseSyncPerf = $state(typeof performance !== 'undefined' ? performance.now() : 0);
  let rafTickMs = $state(playerStore.positionSec * 1000);

  $effect(() => {
    baseAudioMs = playerStore.positionSec * 1000;
    baseSyncPerf = performance.now();
    rafTickMs = baseAudioMs;
  });

  $effect(() => {
    if (!playerStore.isPlaying || playerStore.isBuffering || !doc?.synced) return;
    let frameId = 0;
    const step = () => {
      rafTickMs = baseAudioMs + (performance.now() - baseSyncPerf) * playerStore.speed;
      frameId = requestAnimationFrame(step);
    };
    frameId = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frameId);
  });

  let posMs = $derived(Math.max(0, rafTickMs + offsetMs));
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs) : 0);
  let activeLine = $derived(doc?.lines[activeIdx]);
  let durationMs = $derived(playerStore.durationSec * 1000);
  let timedText = $derived(doc ? toTimedText(doc, durationMs, featuresStore.wordByWord) : null);
  let enhancedReady = $state(false);
  let lyricsView = $state<HTMLDivElement | null>(null);
  let lastManualScroll = $state(-Infinity);
  let userScrolled = $derived(typeof performance !== 'undefined' && performance.now() - lastManualScroll < 3000);

  $effect(() => {
    if (!timedText || enhancedReady) return;
    let mounted = true;
    void import('@uimaxbai/am-lyrics/am-lyrics.js').then(() => {
      if (mounted) enhancedReady = true;
    }).catch((error) => console.warn('Synced lyrics renderer failed to load:', error));
    return () => { mounted = false; };
  });

  function configureRenderer(element: AmLyrics) {
    keepScrollOnSungWord(element);
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
      /* Keep the intro spacing in the padding, where the dot animation cannot collapse it. */
      :host .lyrics-container:has(> #gap-0) {
        --lw-lyrics-intro-space: calc(var(--am-lyrics-instrumental-height) + var(--am-lyrics-instrumental-spacing));
        padding-block-start: calc(var(--lw-lyrics-top-padding) + var(--lw-lyrics-intro-space));
      }
      :host #gap-0.lyrics-gap {
        height: 0;
        pointer-events: none;
      }
      :host #gap-0.lyrics-gap .main-vocal-container {
        inset-block: auto 0;
      }
      :host(.sidebar-renderer) .lyrics-container {
        --lw-lyrics-top-padding: 22px;
        /* Reserve space so opening a translation keeps the final line visible. */
        padding-block: var(--lw-lyrics-top-padding) calc(var(--lw-lyrics-translation-max) + var(--lw-lyrics-edge-fade));
        scrollbar-width: none;
      }
      :host(.sidebar-renderer) .lyrics-container::-webkit-scrollbar,
      :host .lyrics-container::-webkit-scrollbar {
        display: none;
        width: 0;
        height: 0;
      }
      :host .lyrics-container {
        --lw-lyrics-top-padding: 60px;
        scrollbar-width: none;
      }
      @keyframes lw-bounce {
        0% { transform: translateY(3px) scale(0.99); }
        50% { transform: translateY(-3px) scale(1.035); }
        100% { transform: translateY(0) scale(1.02); }
      }
      @keyframes lw-pop {
        0% { transform: scale(0.96); }
        55% { transform: scale(1.06); }
        100% { transform: scale(1.03); }
      }
      @keyframes lw-warp {
        0% { transform: scale(0.94); letter-spacing: -0.02em; }
        60% { transform: scale(1.05); letter-spacing: 0.03em; }
        100% { transform: scale(1.03); letter-spacing: 0.015em; }
      }
      @keyframes lw-glitch {
        0%, 100% { transform: translateX(0); text-shadow: -2px 0 rgba(255, 45, 85, 0.8), 2px 0 rgba(0, 229, 255, 0.8); }
        25% { transform: translateX(-2px); }
        50% { transform: translateX(2px); }
      }
      :host .lyrics-line.active {
        transform-origin: left center;
        transition: transform 280ms cubic-bezier(0.22, 1, 0.36, 1), text-shadow 260ms ease, filter 260ms ease;
      }
      :host([data-animation="karaoke_bounce"]) .lyrics-line.active { animation: lw-bounce 420ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
      :host([data-animation="kinetic_pop"]) .lyrics-line.active { animation: lw-pop 340ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
      :host([data-animation="neon_cyber"]) .lyrics-line.active { transform: scale(1.025); text-shadow: 0 0 22px color-mix(in srgb, var(--lw-accent) 85%, transparent); }
      :host([data-animation="italic_wave"]) .lyrics-line.active { font-style: italic; transform: skewX(-5deg) scale(1.025); }
      :host([data-animation="glitch_snap"]) .lyrics-line.active { animation: lw-glitch 260ms steps(2, end) forwards; }
      :host([data-animation="elastic_tilt"]) .lyrics-line.active { transform: perspective(520px) rotateX(7deg) scale(1.03); }
      :host([data-animation="hyper_warp"]) .lyrics-line.active { animation: lw-warp 360ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
      :host([data-animation="chaos_vortex"]) .lyrics-line.active { transform: rotate(-1.2deg) scale(1.045); }
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
  <div
    class="synced-lyrics"
    class:compact
    data-lw="lyrics-view"
    data-animation={featuresStore.lyricsAnimation}
    style:--lw-wide-override={fontSize ? `${fontSize}px` : undefined}
  >
    {#key doc.trackId}
      <am-lyrics
        use:configureRenderer
        class:sidebar-renderer={compact}
        data-animation={featuresStore.lyricsAnimation}
        ttml={timedText}
        currentTime={posMs}
        duration={durationMs || undefined}
        paused={!playerStore.isPlaying || playerStore.isBuffering}
        interpolate={playerStore.speed === 1}
        noAutoAlternates={true}
        noBlur={settingsStore.reduceMotion}
        reducedMotion={settingsStore.reduceMotion}
        fontFamily="var(--lw-font-family)"
        style:--lw-lyrics-scale={featuresStore.lyricsFontScale}
        style:--am-lyrics-wide-font-size={fontSize ? `calc(${fontSize}px * var(--lw-lyrics-scale))` : undefined}
      ></am-lyrics>
    {/key}
    {#if showHeaderControls || (featuresStore.showTranslation && activeLine?.translation) || (featuresStore.showRomanized && activeLine?.romanized)}
      <footer class="lyrics-footer">
        {#if (featuresStore.showTranslation && activeLine?.translation) || (featuresStore.showRomanized && activeLine?.romanized)}
          <div class="active-translation lw-scroll">
            {#if featuresStore.showRomanized && activeLine?.romanized}<div>{activeLine.romanized}</div>{/if}
            {#if featuresStore.showTranslation && activeLine?.translation}<div>{activeLine.translation}</div>{/if}
          </div>
        {/if}
        {#if showHeaderControls}
          <div class="lyrics-meta">
            <span>Source: {doc.provider}</span>
            {#if onShareLine && activeLine?.text}
              <button type="button" onclick={shareActiveLine} use:waveTooltip={'Share lyric card'}>Share line</button>
            {/if}
          </div>
        {/if}
      </footer>
    {/if}
  </div>
{:else}
<div
  bind:this={lyricsView}
  onwheel={() => (lastManualScroll = performance.now())}
  data-lw="lyrics-view"
  data-animation={featuresStore.lyricsAnimation}
  class="lyrics-fallback mx-auto w-full max-w-[660px] py-4"
  class:compact
  class:lw-scroll={compact}
  style:--lw-lyrics-scale={featuresStore.lyricsFontScale}
  style:--lw-wide-override={fontSize ? `${fontSize}px` : undefined}
>
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
            use:waveTooltip={'Share lyric card'}
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
    mask-image: linear-gradient(
      to bottom,
      transparent,
      black var(--lw-lyrics-top-fade, min(42px, 12%)),
      black calc(100% - var(--lw-lyrics-edge-fade, min(28px, 10%))),
      transparent
    );
    --am-lyrics-highlight-color: var(--lw-text-primary);
    --am-lyrics-highlight-surface: transparent;
    --am-lyrics-highlight-radius: 10px;
    --am-lyrics-inactive-scale: 1;
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
    --lw-lyrics-top-fade: 10px;
    --lw-lyrics-edge-fade: 22px;
  }
  .compact am-lyrics {
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
  @keyframes fb-bounce {
    0% { transform: translateY(3px) scale(0.99); }
    50% { transform: translateY(-3px) scale(1.035); }
    100% { transform: translateY(0) scale(1.02); }
  }
  @keyframes fb-pop {
    0% { transform: scale(0.96); }
    55% { transform: scale(1.06); }
    100% { transform: scale(1.03); }
  }
  @keyframes fb-warp {
    0% { transform: scale(0.94); letter-spacing: -0.02em; }
    60% { transform: scale(1.05); letter-spacing: 0.03em; }
    100% { transform: scale(1.03); letter-spacing: 0.015em; }
  }
  @keyframes fb-glitch {
    0%, 100% { transform: translateX(0); text-shadow: -2px 0 rgba(255, 45, 85, 0.8), 2px 0 rgba(0, 229, 255, 0.8); }
    25% { transform: translateX(-2px); }
    50% { transform: translateX(2px); }
  }
  .lyric-line[data-active="true"] {
    color: var(--lw-text-primary);
    transform-origin: left center;
    transition: transform var(--lw-motion-normal) var(--ease-wave), color var(--lw-motion-normal);
  }
  [data-animation="karaoke_bounce"] .lyric-line[data-active="true"] { animation: fb-bounce 420ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
  [data-animation="kinetic_pop"] .lyric-line[data-active="true"] { animation: fb-pop 340ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
  [data-animation="neon_cyber"] .lyric-line[data-active="true"] { transform: scale(1.025); text-shadow: 0 0 22px color-mix(in srgb, var(--lw-accent) 85%, transparent); }
  [data-animation="italic_wave"] .lyric-line[data-active="true"] { font-style: italic; transform: skewX(-5deg) scale(1.025); }
  [data-animation="glitch_snap"] .lyric-line[data-active="true"] { animation: fb-glitch 260ms steps(2, end) forwards; }
  [data-animation="elastic_tilt"] .lyric-line[data-active="true"] { transform: perspective(520px) rotateX(7deg) scale(1.03); }
  [data-animation="hyper_warp"] .lyric-line[data-active="true"] { animation: fb-warp 360ms cubic-bezier(0.22, 1, 0.36, 1) forwards; }
  [data-animation="chaos_vortex"] .lyric-line[data-active="true"] { transform: rotate(-1.2deg) scale(1.045); }
  .lyric-text { font-size: calc(23px * var(--lw-lyrics-scale)); font-weight: 650; line-height: 1.36; overflow-wrap: anywhere; }
  .compact .lyric-text { font-size: calc(var(--lw-sidebar-lyric-size) * var(--lw-lyrics-scale)); line-height: 1.22; }
  @media (hover: hover) and (pointer: fine) {
    .lyric-line[data-active="false"]:hover { background: var(--lw-wash-hover); color: var(--lw-text-primary); }
  }
  @media (forced-colors: active) {
    am-lyrics { mask-image: none; }
  }
</style>
