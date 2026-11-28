<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { windowState } from '$src/app/window.svelte';
  import { activeLineIndex } from '$src/features/lyrics/lyricAdapter';
  import { isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveSlider from '$src/ui/components/WaveSlider.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSlide } from '$src/ui/theme/motion';
  import { WaveMotion } from '$src/ui/theme/tokens';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  let hover = $state(false);
  let seekPreview = $state<number | null>(null);
  let track = $derived(playerStore.currentTrack);
  let style = $derived(settingsStore.miniPlayerStyle);
  let duration = $derived(playerStore.durationSec);
  let seekPosition = $derived(seekPreview ?? playerStore.positionSec);

  let lyricsDoc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let hasSyncedLyrics = $derived(Boolean(lyricsDoc && lyricsDoc.synced && lyricsDoc.lines.length > 0));
  let showLyrics = $derived(style === 'lyrics' && hasSyncedLyrics);

  let activeLyricText = $derived.by(() => {
    if (!lyricsDoc || !hasSyncedLyrics) return '';
    const offsetMs = track ? featuresStore.getLyricsOffset(track.id) : 0;
    const tMs = Math.max(0, playerStore.positionSec * 1000 + offsetMs);
    return lyricsDoc.lines[activeLineIndex(lyricsDoc.lines, tMs)]?.text ?? '';
  });

  $effect(() => {
    track?.id;
    style;
    seekPreview = null;
  });

  $effect(() => {
    if (windowState.isMiniPlayer && style === 'lyrics') {
      windowState.syncMiniPlayerBounds(hasSyncedLyrics);
    }
  });

  function expand() {
    windowState.toggleMiniPlayer();
  }

  function closeMini() {
    if (appWindow) {
      void appWindow.close();
    } else {
      expand();
    }
  }

  function onSurfaceDblClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.closest('button, input')) return;
    expand();
  }

  function commitSeek(position: number) {
    playerStore.seek(position);
    seekPreview = null;
  }
</script>

{#snippet playButton(size: number, iconSize: number, onDark = false)}
  <button
    type="button"
    onclick={() => playerStore.togglePlay()}
    use:waveTooltip={playerStore.isPlaying ? 'Pause' : 'Play'}
    aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
    class="mini-play lw-press"
    class:on-dark={onDark}
    style:--mini-play-size={`${size}px`}
    data-playing={playerStore.isPlaying}
  >
    <span class="mini-play-icons" aria-hidden="true">
      <span><WaveIcon name="play" size={iconSize} filled /></span>
      <span><WaveIcon name="pause" size={iconSize} filled /></span>
    </span>
  </button>
{/snippet}

{#snippet seekSlider()}
  <div class="mini-seek flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-white/75 lw-tabular">
    <span>{formatDuration(seekPosition)}</span>
    <div role="group" class="min-w-0 flex-1" onpointercancel={() => (seekPreview = null)} onfocusout={() => (seekPreview = null)}>
      <WaveSlider
        label="Seek track position"
        valuetext={formatDuration(seekPosition)}
        max={duration || 1}
        step={0.1}
        value={seekPosition}
        disabled={!duration}
        oninput={(position) => (seekPreview = position)}
        onchange={commitSeek}
      />
    </div>
    <span>{formatDuration(duration)}</span>
  </div>
{/snippet}

<div
  data-lw="mini-player"
  role="region"
  aria-label="Mini Player"
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  ondblclick={onSurfaceDblClick}
  data-tauri-drag-region
  class="relative flex h-full w-full flex-col overflow-hidden border select-none {track && (style === 'card' || style === 'ambient' || style === 'lyrics')
    ? 'border-white/10 bg-black text-white'
    : 'border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] text-[var(--lw-text-primary)]'}"
>
  {#if !track}
    <div class="flex h-full items-center gap-2 px-2.5" data-tauri-drag-region>
      <WaveIcon name="music" size={15} class="shrink-0 text-[var(--lw-text-secondary)]" />
      <button
        type="button"
        onclick={expand}
        class="min-w-0 flex-1 cursor-pointer truncate text-left text-[12px] text-[var(--lw-text-secondary)]"
      >
        No track playing
      </button>
      <div class="flex items-center">
        <button type="button" onclick={() => appWindow?.minimize()} use:waveTooltip={'Minimize'} aria-label="Minimize" class="mini-glyph">
          <span class="h-[1px] w-2 bg-current"></span>
        </button>
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand" class="mini-glyph">
          <WaveIcon name="maximize" size={12} />
        </button>
        <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph danger">
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    </div>
  {:else if style === 'card'}
    <div class="relative h-full w-full" data-tauri-drag-region>
      {#key track.artworkUrl}
        <img src={track.artworkUrl} alt={track.title} class="absolute inset-0 h-full w-full object-cover" draggable="false" in:waveSlide={{}} />
      {/key}
      <div
        class="mini-card-shade pointer-events-none absolute inset-0"
      ></div>
      <div class="mini-card-top-shade pointer-events-none absolute inset-0" class:visible={hover}></div>

      <div class="mini-window-controls card-controls absolute top-1.5 right-1.5 flex items-center gap-0.5" class:visible={hover}>
        <button type="button" onclick={() => appWindow?.minimize()} use:waveTooltip={'Minimize'} aria-label="Minimize" class="mini-glyph on-dark">
          <span class="h-[1px] w-2 bg-current"></span>
        </button>
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand" class="mini-glyph on-dark">
          <WaveIcon name="maximize" size={12} />
        </button>
        <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph on-dark danger">
          <WaveIcon name="close" size={12} />
        </button>
      </div>

      <div class="absolute inset-x-3 bottom-2.5 space-y-1.5">
        <button type="button" onclick={expand} class="block w-full cursor-pointer text-left">
          <div class="truncate text-[14px] font-bold text-white">{track.title}</div>
          <div class="truncate text-[11.5px] text-white/75">{track.artist}</div>
        </button>

        {@render seekSlider()}

        <div class="flex items-center justify-center gap-2">
          <button type="button" onclick={() => playerStore.previous()} use:waveTooltip={'Previous'} aria-label="Previous" class="mini-glyph on-dark">
            <WaveIcon name="prev" size={14} />
          </button>
          {@render playButton(36, 16, true)}
          <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph on-dark">
            <WaveIcon name="next" size={14} />
          </button>
        </div>
      </div>
    </div>
  {:else if style === 'ambient' || style === 'lyrics'}
    <div class="relative flex h-full w-full flex-col overflow-hidden" data-tauri-drag-region>
      {#if track.artworkUrl}
        {#key track.artworkUrl}
          <div class="pointer-events-none absolute inset-0" aria-hidden="true" in:waveSlide={{}}>
            <img src={track.artworkUrl} alt="" class="h-full w-full scale-125 object-cover blur-2xl" draggable="false" />
          </div>
        {/key}
      {/if}
      <div class="mini-ambient-shade pointer-events-none absolute inset-0"></div>

      <div class="relative z-10 flex flex-1 items-center gap-3 px-3 py-2.5" data-tauri-drag-region>
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand to window" class="shrink-0 cursor-pointer rounded-[10px] lw-press">
          {#key track.artworkUrl}
            <div in:waveSlide={{}}>
              <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={showLyrics ? 96 : 86} radius={10} shadow="lift" />
            </div>
          {/key}
        </button>

        <div class="flex min-w-0 flex-1 flex-col justify-between self-stretch">
          <div class="flex items-start justify-between gap-2" data-tauri-drag-region>
            <button type="button" onclick={expand} class="min-w-0 flex-1 cursor-pointer text-left">
              <div class="truncate text-[13.5px] font-bold text-white">{track.title}</div>
              <div class="truncate text-[11.5px] text-white/75">{track.artist}</div>
            </button>
            <div class="mini-window-controls flex shrink-0 items-center" class:visible={hover}>
              <button type="button" onclick={() => appWindow?.minimize()} use:waveTooltip={'Minimize'} aria-label="Minimize" class="mini-glyph on-dark">
                <span class="h-[1px] w-2 bg-current"></span>
              </button>
              <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand" class="mini-glyph on-dark">
                <WaveIcon name="maximize" size={12} />
              </button>
              <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph on-dark danger">
                <WaveIcon name="close" size={12} />
              </button>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <button type="button" onclick={() => playerStore.previous()} use:waveTooltip={'Previous'} aria-label="Previous" class="mini-glyph on-dark">
              <WaveIcon name="prev" size={13} />
            </button>
            {@render playButton(32, 15, true)}
            <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph on-dark">
              <WaveIcon name="next" size={13} />
            </button>

            {@render seekSlider()}
          </div>

          {#if showLyrics}
            {@const lyric = activeLyricText || '♪'}
            {@const duration = playerStore.isPlaying && !playerStore.isBuffering ? WaveMotion.fastMs : 0}
            <div class="mini-lyrics-line overflow-hidden rounded-[6px] bg-white/10 px-2 py-1 text-[11.5px] font-medium leading-4 text-white/95">
              <span class="sr-only">{lyric}</span>
              {#key lyric}
                <span
                  aria-hidden="true"
                  class="truncate"
                  in:waveSlide={{ y: 3, duration }}
                  out:waveSlide={{ y: -3, duration }}
                >{lyric}</span>
              {/key}
            </div>
          {/if}
        </div>
      </div>
    </div>
  {:else if style === 'compact'}
    <div class="flex flex-1 items-center gap-2.5 px-2" data-tauri-drag-region>
      <button type="button" onclick={expand} class="shrink-0 cursor-pointer">
        <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={36} radius={8} />
      </button>
      <button type="button" onclick={expand} class="min-w-0 flex-1 cursor-pointer text-left">
        <div class="truncate text-[12px] font-semibold">{track.title}</div>
        <div class="truncate text-[11px] text-[var(--lw-text-secondary)]">{track.artist}</div>
      </button>
      <div class="flex shrink-0 items-center gap-0.5">
        {@render playButton(28, 13)}
        <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph">
          <WaveIcon name="next" size={13} />
        </button>
      </div>
      <div class="mini-window-controls compact-controls flex shrink-0 items-center" class:visible={hover}>
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand to window" class="mini-glyph">
          <WaveIcon name="maximize" size={12} />
        </button>
        <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph danger">
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    </div>
    <div class="h-[2px] w-full bg-[var(--lw-wash-hover)]" aria-hidden="true">
      <div class="mini-progress-fill" style:transform={`scaleX(${playerStore.progress})`}></div>
    </div>
  {:else}
    <!-- classic (380x78) -->
    <div class="flex flex-1 items-center gap-2 px-2" data-tauri-drag-region>
      <span class="pointer-events-none flex h-6 w-3 shrink-0 items-center justify-center text-[var(--lw-text-tertiary)] opacity-55">
        ⋮
      </span>
      <button type="button" onclick={expand} class="shrink-0 cursor-pointer">
        <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={46} radius={8} />
      </button>
      <button type="button" onclick={expand} class="min-w-0 flex-1 cursor-pointer text-left">
        <div class="truncate text-[12px] font-semibold">{track.title}</div>
        <div class="truncate text-[11px] text-[var(--lw-text-secondary)]">{track.artist}</div>
      </button>
      <div class="flex shrink-0 items-center gap-0.5">
        <button type="button" onclick={() => playerStore.previous()} use:waveTooltip={'Previous'} aria-label="Previous" class="mini-glyph">
          <WaveIcon name="prev" size={13} />
        </button>
        {@render playButton(32, 14)}
        <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph">
          <WaveIcon name="next" size={13} />
        </button>
      </div>
      <div class="mx-0.5 h-4 w-px bg-[var(--lw-divider)]"></div>
      <div class="flex shrink-0 items-center">
        <button type="button" onclick={() => appWindow?.minimize()} use:waveTooltip={'Minimize'} aria-label="Minimize" class="mini-glyph">
          <span class="h-[1px] w-2 bg-current"></span>
        </button>
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand" class="mini-glyph">
          <WaveIcon name="maximize" size={12} />
        </button>
        <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph danger">
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    </div>
    <div class="h-[3px] w-full bg-[var(--lw-wash-hover)]" aria-hidden="true">
      <div class="mini-progress-fill" style:transform={`scaleX(${playerStore.progress})`}></div>
    </div>
  {/if}
</div>

<style>
  .mini-ambient-shade {
    background: linear-gradient(110deg, rgb(0 0 0 / 48%), rgb(0 0 0 / 26%)),
      linear-gradient(to top, rgb(0 0 0 / 30%), transparent 75%);
  }

  .mini-card-shade {
    background: linear-gradient(to top, rgb(0 0 0 / 88%), transparent 58%);
  }

  .mini-card-top-shade {
    background: linear-gradient(to bottom, rgb(0 0 0 / 55%), transparent 30%);
    opacity: 0;
    transition: opacity var(--lw-motion-normal) var(--ease-wave);
  }

  .mini-window-controls {
    opacity: 0.65;
    transition: opacity var(--lw-motion-normal) var(--ease-wave);
  }

  .compact-controls { opacity: 0.55; }
  .card-controls { opacity: 0; pointer-events: none; }
  .mini-card-top-shade.visible,
  .mini-window-controls:is(.visible, :focus-within) { opacity: 1; }
  .card-controls:is(.visible, :focus-within) { pointer-events: auto; }

  .mini-play {
    display: grid;
    width: var(--mini-play-size);
    height: var(--mini-play-size);
    flex: none;
    place-items: center;
    border-radius: 50%;
    background: var(--lw-accent);
    color: var(--lw-on-accent);
    cursor: pointer;
  }

  .mini-play.on-dark { background: white; color: black; }
  .mini-play-icons { display: grid; place-items: center; line-height: 1; }
  .mini-play-icons > span {
    grid-area: 1 / 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    transition: opacity var(--lw-motion-fast) var(--ease-wave);
  }

  .mini-play[data-playing="false"] .mini-play-icons > span:last-child,
  .mini-play[data-playing="true"] .mini-play-icons > span:first-child { opacity: 0; }

  .mini-progress-fill {
    display: block;
    height: 100%;
    background: var(--lw-accent);
    transform-origin: left center;
    transition: transform var(--lw-motion-fast) linear;
  }

  .mini-seek {
    --lw-text-primary: #fff;
    --lw-outline: rgb(255 255 255 / 28%);
  }

  .mini-lyrics-line { display: grid; }
  .mini-lyrics-line > .truncate { min-width: 0; grid-area: 1 / 1; }

  .mini-glyph {
    display: flex;
    width: 24px;
    height: 24px;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast),
      scale var(--lw-motion-fast) var(--ease-wave);
  }

  .mini-glyph.on-dark {
    color: rgba(255, 255, 255, 0.85);
  }

  .mini-glyph:active { scale: calc(1 - 0.035 * var(--lw-motion-factor)); }

  @media (hover: hover) and (pointer: fine) {
    .mini-play:hover { background: color-mix(in srgb, var(--lw-accent) 88%, var(--lw-text-primary)); }
    .mini-play.on-dark:hover { background: rgb(255 255 255 / 90%); }

    .mini-glyph:hover {
      background: var(--lw-wash-hover);
      color: var(--lw-text-primary);
    }

    .mini-glyph.on-dark:hover {
      background: rgb(255 255 255 / 14%);
      color: white;
    }

    .mini-glyph.danger:hover {
      background: var(--lw-danger);
      color: white;
    }
  }
</style>
