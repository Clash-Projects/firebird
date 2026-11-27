<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { windowState } from '$src/app/window.svelte';
  import { isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveSlider from '$src/ui/components/WaveSlider.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  let hover = $state(false);
  let track = $derived(playerStore.currentTrack);
  let style = $derived(settingsStore.miniPlayerStyle);
  let progressPct = $derived(Math.max(0, Math.min(100, playerStore.progress * 100)));

  let lyricsDoc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let hasSyncedLyrics = $derived(Boolean(lyricsDoc && lyricsDoc.synced && lyricsDoc.lines.length > 0));

  let activeLyricText = $derived.by(() => {
    if (!lyricsDoc || !hasSyncedLyrics) return '';
    const offsetMs = track ? featuresStore.getLyricsOffset(track.id) : 0;
    const tMs = Math.max(0, playerStore.positionSec * 1000 + offsetMs);
    let active = lyricsDoc.lines[0]?.text ?? '';
    for (const line of lyricsDoc.lines) {
      if (line.startMs <= tMs) active = line.text;
      else break;
    }
    return active;
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
</script>

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
      <img src={track.artworkUrl} alt={track.title} class="h-full w-full object-cover" draggable="false" />
      <div
        class="pointer-events-none absolute inset-0 transition-opacity duration-150"
        style:background="linear-gradient(to bottom, rgba(0,0,0,{hover ? 0.55 : 0}) 0%, rgba(0,0,0,0) 30%, rgba(0,0,0,0) 42%, rgba(0,0,0,0.88) 100%)"
      ></div>

      <div class="absolute top-1.5 right-1.5 flex items-center gap-0.5 transition-opacity duration-150 {hover ? 'opacity-100' : 'pointer-events-none opacity-0'}">
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

        <div class="mini-on-dark flex items-center gap-2 text-[10px] text-white/75 lw-tabular">
          <span>{formatDuration(playerStore.positionSec)}</span>
          <div class="min-w-0 flex-1">
            <WaveSlider
              min={0}
              max={playerStore.durationSec || 1}
              step={1}
              label="Seek"
              value={playerStore.positionSec}
              oninput={(v) => playerStore.seek(v)}
            />
          </div>
          <span>{formatDuration(playerStore.durationSec)}</span>
        </div>

        <div class="flex items-center justify-center gap-2">
          <button type="button" onclick={() => playerStore.previous()} use:waveTooltip={'Previous'} aria-label="Previous" class="mini-glyph on-dark">
            <WaveIcon name="prev" size={14} />
          </button>
          <button
            type="button"
            onclick={() => playerStore.togglePlay()}
            use:waveTooltip={playerStore.isPlaying ? 'Pause' : 'Play'}
            aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
            class="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-white text-black transition-transform hover:scale-105 active:scale-95"
          >
            <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={16} filled />
          </button>
          <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph on-dark">
            <WaveIcon name="next" size={14} />
          </button>
        </div>
      </div>
    </div>
  {:else if style === 'ambient' || style === 'lyrics'}
    <div class="relative flex h-full w-full flex-col overflow-hidden" data-tauri-drag-region>
      {#if track.artworkUrl}
        <img
          src={track.artworkUrl}
          alt=""
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 h-full w-full scale-125 object-cover blur-2xl"
        />
      {/if}
      <div class="pointer-events-none absolute inset-0 bg-black/45"></div>

      <div class="relative z-10 flex flex-1 items-center gap-3 px-3 py-2.5" data-tauri-drag-region>
        <button type="button" onclick={expand} class="shrink-0 cursor-pointer">
          <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={hasSyncedLyrics ? 96 : 86} radius={10} shadow="lift" />
        </button>

        <div class="flex min-w-0 flex-1 flex-col justify-between self-stretch">
          <div class="flex items-start justify-between gap-2" data-tauri-drag-region>
            <button type="button" onclick={expand} class="min-w-0 flex-1 cursor-pointer text-left">
              <div class="truncate text-[13.5px] font-bold text-white">{track.title}</div>
              <div class="truncate text-[11.5px] text-white/75">{track.artist}</div>
            </button>
            <div class="flex shrink-0 items-center transition-opacity duration-150 {hover ? 'opacity-100' : 'opacity-60'}">
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
            <button
              type="button"
              onclick={() => playerStore.togglePlay()}
              use:waveTooltip={playerStore.isPlaying ? 'Pause' : 'Play'}
              aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
              class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-full bg-white text-black transition-transform hover:scale-105 active:scale-95"
            >
              <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={15} filled />
            </button>
            <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph on-dark">
              <WaveIcon name="next" size={13} />
            </button>

            <div class="mini-on-dark ml-1 flex min-w-0 flex-1 items-center gap-1.5 text-[10px] text-white/75 lw-tabular">
              <span>{formatDuration(playerStore.positionSec)}</span>
              <div class="min-w-0 flex-1">
                <WaveSlider
                  min={0}
                  max={playerStore.durationSec || 1}
                  step={1}
                  label="Seek"
                  value={playerStore.positionSec}
                  oninput={(v) => playerStore.seek(v)}
                />
              </div>
              <span>{formatDuration(playerStore.durationSec)}</span>
            </div>
          </div>

          {#if style === 'lyrics' && hasSyncedLyrics}
            <div class="truncate rounded-[6px] bg-white/10 px-2 py-1 text-[11.5px] font-medium text-white/95">
              {activeLyricText || '♪'}
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
        <button
          type="button"
          onclick={() => playerStore.togglePlay()}
          use:waveTooltip={playerStore.isPlaying ? 'Pause' : 'Play'}
          aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
          class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)]"
        >
          <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={13} filled />
        </button>
        <button type="button" onclick={() => playerStore.next()} use:waveTooltip={'Next'} aria-label="Next" class="mini-glyph">
          <WaveIcon name="next" size={13} />
        </button>
      </div>
      <div class="flex shrink-0 items-center transition-opacity duration-150 {hover ? 'opacity-100' : 'opacity-45'}">
        <button type="button" onclick={expand} use:waveTooltip={'Expand to window'} aria-label="Expand" class="mini-glyph">
          <WaveIcon name="maximize" size={12} />
        </button>
        <button type="button" onclick={closeMini} use:waveTooltip={'Close mini player'} aria-label="Close" class="mini-glyph danger">
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    </div>
    <div class="h-[2px] w-full bg-[var(--lw-wash-hover)]">
      <div class="h-full bg-[var(--lw-accent)] transition-[width] duration-150" style:width="{progressPct}%"></div>
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
        <button
          type="button"
          onclick={() => playerStore.togglePlay()}
          use:waveTooltip={playerStore.isPlaying ? 'Pause' : 'Play'}
          aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] transition-transform hover:scale-105 active:scale-95"
        >
          <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={14} filled />
        </button>
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
    <div class="h-[3px] w-full bg-[var(--lw-wash-hover)]">
      <div class="h-full bg-[var(--lw-accent)] transition-[width] duration-150" style:width="{progressPct}%"></div>
    </div>
  {/if}
</div>

<style>
  .mini-on-dark {
    --lw-accent: #ffffff;
    --lw-outline-strong: rgba(255, 255, 255, 0.3);
  }

  .mini-glyph {
    display: flex;
    width: 24px;
    height: 24px;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: background-color 110ms ease, color 110ms ease;
  }

  .mini-glyph.on-dark {
    color: rgba(255, 255, 255, 0.85);
  }

  .mini-glyph:hover {
    background: var(--lw-wash-hover);
    color: var(--lw-text-primary);
  }

  .mini-glyph.on-dark:hover {
    background: rgba(255, 255, 255, 0.14);
    color: #fff;
  }

  .mini-glyph.danger:hover {
    background: var(--lw-danger);
    color: #fff;
  }
</style>
