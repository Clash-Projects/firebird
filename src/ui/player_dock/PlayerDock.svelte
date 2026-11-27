<script lang="ts">
  import { dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { toggleTrackLike } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);

  let seekDragging = $state(false);
  let seekDragSec = $state<number | null>(null);
  let seekHoverRatio = $state<number | null>(null);
  let seekBarEl = $state<HTMLDivElement | null>(null);

  let volDragging = $state(false);
  let volHover = $state(false);
  let volBarEl = $state<HTMLDivElement | null>(null);

  let qualityOpen = $state(false);

  let position = $derived(seekDragSec ?? playerStore.positionSec);
  let duration = $derived(playerStore.durationSec);
  let seekRatio = $derived(duration > 0 ? Math.min(1, Math.max(0, position / duration)) : 0);
  let seekPct = $derived(seekRatio * 100);
  let hoverTimeLabel = $derived(
    seekHoverRatio !== null && duration > 0 ? formatDuration(seekHoverRatio * duration) : null
  );

  let volRatio = $derived(playerStore.muted ? 0 : Math.min(1, Math.max(0, playerStore.volume)));
  let volPct = $derived(Math.round(volRatio * 100));
  let volumeIcon = $derived<WaveIconName>(
    playerStore.muted || playerStore.volume === 0
      ? 'volumeMute'
      : playerStore.volume < 0.5
        ? 'volumeLow'
        : 'volumeHigh'
  );

  let qualityBadge = $derived.by(() => {
    if (!track) return '';
    if (track.qualityTier === 'atmos' || track.codec.toUpperCase().includes('ATMOS')) return 'ATMOS';
    if (track.bitDepth >= 24 || track.sampleRateHz > 48000) return 'HI-RES';
    if (track.codec.toUpperCase() === 'FLAC' || track.codec.toUpperCase() === 'ALAC') return 'LOSSLESS';
    return track.codec.toUpperCase() || 'AUDIO';
  });

  // Wavy seekbar SVG path along played portion
  let wavyPath = $derived.by(() => {
    if (!settingsStore.wavySeekbar || seekPct <= 0.5) return '';
    const w = 200;
    const endX = (seekPct / 100) * w;
    const amp = playerStore.isPlaying ? 2.4 : 0.8;
    const freq = 0.22;
    let d = `M 0 6`;
    for (let x = 2; x <= endX; x += 2) {
      const y = 6 + Math.sin(x * freq) * amp;
      d += ` L ${x.toFixed(1)} ${y.toFixed(2)}`;
    }
    return d;
  });

  function ratioFromClientX(el: HTMLDivElement | null, clientX: number): number {
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  }

  function onSeekPointerDown(e: PointerEvent) {
    if (!track || duration <= 0 || !seekBarEl) return;
    e.currentTarget && (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    seekDragging = true;
    const r = ratioFromClientX(seekBarEl, e.clientX);
    seekDragSec = r * duration;
    seekHoverRatio = r;
  }

  function onSeekPointerMove(e: PointerEvent) {
    if (!track || duration <= 0 || !seekBarEl) return;
    const r = ratioFromClientX(seekBarEl, e.clientX);
    seekHoverRatio = r;
    if (seekDragging) {
      seekDragSec = r * duration;
    }
  }

  function onSeekPointerUp(e: PointerEvent) {
    if (!seekDragging) return;
    const r = ratioFromClientX(seekBarEl, e.clientX);
    seekDragging = false;
    seekDragSec = null;
    if (duration > 0) {
      playerStore.seek(r * duration);
    }
  }

  function onSeekKeyDown(e: KeyboardEvent) {
    if (!track || duration <= 0) return;
    const step = e.shiftKey ? 15 : 5;
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      playerStore.seek(Math.min(duration, playerStore.positionSec + step));
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      playerStore.seek(Math.max(0, playerStore.positionSec - step));
    }
  }

  function onVolPointerDown(e: PointerEvent) {
    if (!volBarEl) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    volDragging = true;
    const r = ratioFromClientX(volBarEl, e.clientX);
    playerStore.setVolume(r);
  }

  function onVolPointerMove(e: PointerEvent) {
    if (!volDragging || !volBarEl) return;
    const r = ratioFromClientX(volBarEl, e.clientX);
    playerStore.setVolume(r);
  }

  function onVolPointerUp() {
    volDragging = false;
  }

  function onVolKeyDown(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
      e.preventDefault();
      playerStore.setVolume(Math.min(1, playerStore.volume + 0.05));
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
      e.preventDefault();
      playerStore.setVolume(Math.max(0, playerStore.volume - 0.05));
    }
  }
</script>

<WaveHaze level="l1" dataLw="player-bar" class="player-dock">
  <!-- LEFT: Track Identity (290px, placeholder when idle) -->
  <div class="dock-left">
    {#if track}
      <button
        type="button"
        class="dock-cover dock-press"
        title="Open Now Playing"
        onclick={() => router.navigate('/now-playing')}
      >
        {#key track.id}
          <div class="lw-route-enter">
            <Artwork src={track.artworkUrl} alt={`Cover art for ${track.title}`} size={48} radius={6} />
          </div>
        {/key}
      </button>

      <button
        type="button"
        class="dock-identity"
        onclick={() => router.navigate('/now-playing')}
      >
        <span class="dock-title" title={track.title}>{track.title}</span>
        <span class="dock-artist" title={track.artist}>{track.artist}</span>
      </button>

      <button
        type="button"
        class="dock-glyph dock-press"
        aria-label={liked ? 'Unlike' : 'Like'}
        title={liked ? 'Unlike' : 'Like'}
        aria-pressed={liked}
        onclick={() => track && toggleTrackLike(track)}
      >
        <WaveIcon name="heart" size={15} filled={liked} />
      </button>

      <button
        type="button"
        class="dock-glyph dock-track-menu dock-press"
        aria-label="More"
        title="More"
        onclick={(event) => track && contextMenu.showForTrack(event, track)}
      >
        <WaveIcon name="more" size={15} />
      </button>
    {:else}
      <div class="dock-idle-art" aria-hidden="true">
        <WaveIcon name="album" size={22} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="dock-title">Nothing playing</div>
        <div class="dock-artist text-[var(--lw-text-tertiary)]">Pick something to start</div>
      </div>
    {/if}
  </div>

  <!-- CENTER: Transport + Custom Seekbar -->
  <div data-lw="player-controls" class="dock-center">
    <div class="dock-transport-row">
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.toggleShuffle()}
        title="Shuffle"
        aria-pressed={playerStore.shuffle}
      >
        <WaveIcon name="shuffle" size={15} />
      </button>

      <button
        type="button"
        class="dock-glyph dock-large dock-press"
        onclick={() => playerStore.previous()}
        title="Previous"
        disabled={!track}
      >
        <WaveIcon name="prev" size={18} filled />
      </button>

      <button
        type="button"
        class="dock-play dock-press"
        onclick={() => playerStore.togglePlay()}
        title={playerStore.isPlaying ? 'Pause (Space)' : 'Play (Space)'}
        aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
        disabled={!track}
      >
        {#if playerStore.isBuffering}
          <span class="dock-spinner" aria-hidden="true"></span>
        {:else}
          <span class="dock-play-icons" aria-hidden="true" data-playing={playerStore.isPlaying}>
            <span><WaveIcon name="play" size={15} filled /></span>
            <span><WaveIcon name="pause" size={15} filled /></span>
          </span>
        {/if}
      </button>

      <button
        type="button"
        class="dock-glyph dock-large dock-press"
        onclick={() => playerStore.next()}
        title="Next"
        disabled={!track}
      >
        <WaveIcon name="next" size={18} filled />
      </button>

      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.cycleRepeat()}
        title={`Repeat ${playerStore.repeat}`}
        aria-pressed={playerStore.repeat !== 'off'}
      >
        <WaveIcon name={playerStore.repeat === 'one' ? 'repeatOne' : 'repeat'} size={15} />
      </button>
    </div>

    <div data-lw="player-seekbar" class="dock-progress lw-tabular">
      <span class="dock-time dock-time-pos">{formatDuration(position)}</span>

      <div
        bind:this={seekBarEl}
        role="slider"
        tabindex={track && duration > 0 ? 0 : -1}
        aria-label="Seek track position"
        aria-valuemin={0}
        aria-valuemax={Math.round(duration)}
        aria-valuenow={Math.round(position)}
        aria-valuetext={formatDuration(position)}
        class="dock-slider"
        class:disabled={!track || duration <= 0}
        class:active={seekDragging || seekHoverRatio !== null}
        onpointerdown={onSeekPointerDown}
        onpointermove={onSeekPointerMove}
        onpointerup={onSeekPointerUp}
        onpointercancel={() => {
          seekDragging = false;
          seekDragSec = null;
        }}
        onpointerleave={() => {
          if (!seekDragging) seekHoverRatio = null;
        }}
        onkeydown={onSeekKeyDown}
      >
        {#if hoverTimeLabel && (seekHoverRatio !== null || seekDragging)}
          <div
            class="dock-seek-tooltip lw-tabular"
            style:left="{(seekHoverRatio ?? seekRatio) * 100}%"
          >
            {hoverTimeLabel}
          </div>
        {/if}

        {#if settingsStore.wavySeekbar && wavyPath}
          <svg class="dock-wavy-svg" viewBox="0 0 200 12" preserveAspectRatio="none" aria-hidden="true">
            <line x1="0" y1="6" x2="200" y2="6" stroke="var(--lw-outline)" stroke-width="3" stroke-linecap="round" />
            <path d={wavyPath} fill="none" stroke="var(--lw-accent)" stroke-width="3" stroke-linecap="round" />
          </svg>
        {:else}
          <div class="dock-slider-track">
            <div class="dock-slider-fill" style:width="{seekPct}%"></div>
          </div>
        {/if}

        {#if track && duration > 0}
          <div class="dock-slider-thumb" style:left="{seekPct}%"></div>
        {/if}
      </div>

      <span class="dock-time dock-time-dur">{formatDuration(duration)}</span>
    </div>
  </div>

  <!-- RIGHT: Quality badge, Panels, Volume, Expand, Mini -->
  <div class="dock-right">
    {#each uiSlots.playerBarControls as control (control.id)}
      <button type="button" class="dock-glyph dock-press" onclick={control.onClick} title={control.label}>
        <WaveIcon name={(control.icon as WaveIconName) || 'plugin'} size={15} />
      </button>
    {/each}

    {#if track && qualityBadge}
      <div class="relative dock-quality-wrap" use:dismissFlyout={{ open: qualityOpen, close: () => (qualityOpen = false) }}>
        <button
          type="button"
          class="dock-quality-pill dock-press"
          aria-expanded={qualityOpen}
          title={`${track.codec} · ${(track.sampleRateHz / 1000).toFixed(track.sampleRateHz % 1000 === 0 ? 0 : 1)} kHz — details`}
          onclick={() => (qualityOpen = !qualityOpen)}
        >
          {qualityBadge}
        </button>

        <div
          data-open={qualityOpen}
          inert={!qualityOpen}
          class="lw-popover absolute right-0 bottom-10 z-40 w-[240px] rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-2.5 text-[12px] shadow-[var(--lw-shadow-flyout)]"
        >
          <div class="flex items-center gap-2 px-1.5 py-1 font-semibold text-[var(--lw-text-primary)]">
            <WaveIcon name="activity" size={14} class="text-[var(--lw-accent)]" />
            <span>{qualityBadge} · {track.codec}</span>
          </div>
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
          <div class="space-y-1 px-1.5 py-0.5 text-[var(--lw-text-secondary)]">
            <div class="flex justify-between">
              <span>Sample rate</span>
              <span class="lw-tabular text-[var(--lw-text-primary)]">
                {(track.sampleRateHz / 1000).toFixed(track.sampleRateHz % 1000 === 0 ? 0 : 1)} kHz
              </span>
            </div>
            <div class="flex justify-between">
              <span>Bit depth</span>
              <span class="lw-tabular text-[var(--lw-text-primary)]">
                {track.qualityTier === 'atmos' ? 'Dolby Atmos' : `${track.bitDepth}-bit`}
              </span>
            </div>
          </div>
        </div>
      </div>
    {/if}

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('lyrics')}
      title="Lyrics (L)"
      aria-pressed={windowState.activeRightPanel === 'lyrics'}
    >
      <WaveIcon name="lyrics" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('queue')}
      title="Queue (Q)"
      aria-pressed={windowState.activeRightPanel === 'queue'}
    >
      <WaveIcon name="queue" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('eq')}
      title="Equalizer"
      aria-pressed={windowState.activeRightPanel === 'eq'}
    >
      <WaveIcon name="eq" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('output')}
      title="Audio output"
      aria-pressed={windowState.activeRightPanel === 'output'}
    >
      <WaveIcon name="speaker" size={15} />
    </button>

    <!-- Custom 84px Fluent Volume Slider -->
    <div class="dock-volume">
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.toggleMute()}
        title={volRatio === 0 ? 'Unmute (M)' : `Mute (${volPct}%)`}
        aria-pressed={playerStore.muted}
      >
        <WaveIcon name={volumeIcon} size={15} />
      </button>

      <div
        bind:this={volBarEl}
        role="slider"
        tabindex={0}
        aria-label="Volume"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={volPct}
        aria-valuetext={`${volPct}%`}
        title={`${volPct}%`}
        class="dock-slider dock-vol-slider"
        class:active={volDragging || volHover}
        onpointerdown={onVolPointerDown}
        onpointermove={onVolPointerMove}
        onpointerup={onVolPointerUp}
        onpointercancel={onVolPointerUp}
        onpointerenter={() => (volHover = true)}
        onpointerleave={() => (volHover = false)}
        onkeydown={onVolKeyDown}
      >
        {#if volHover || volDragging}
          <div class="dock-seek-tooltip lw-tabular" style:left="{volPct}%">{volPct}%</div>
        {/if}
        <div class="dock-slider-track">
          <div class="dock-slider-fill" style:width="{volPct}%"></div>
        </div>
        <div class="dock-slider-thumb" style:left="{volPct}%"></div>
      </div>
    </div>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => router.navigate('/now-playing')}
      title="Now Playing"
      aria-pressed={router.matches('/now-playing')}
    >
      <WaveIcon name="maximize" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleMiniPlayer()}
      title="Mini player"
    >
      <WaveIcon name="minimize" size={15} />
    </button>
  </div>
</WaveHaze>

<style>
  :global([data-lw="player-bar"].player-dock) {
    position: relative;
    z-index: 20;
    display: flex;
    height: var(--lw-dock-h, 80px);
    flex: none;
    align-items: center;
    gap: 12px;
    padding-inline: 12px;
    border-top: 1px solid var(--lw-divider);
    background: var(--lw-dock-translucent);
  }

  /* LEFT: 290px identity */
  .dock-left {
    display: flex;
    width: 290px;
    min-width: 0;
    flex: none;
    align-items: center;
    gap: 8px;
  }
  .dock-cover { flex: none; cursor: pointer; }
  .dock-identity {
    min-width: 0;
    flex: 1;
    cursor: pointer;
    text-align: left;
  }
  .dock-idle-art {
    display: grid;
    width: 48px;
    height: 48px;
    flex: none;
    place-items: center;
    border: 1px solid var(--lw-outline);
    border-radius: 6px;
    background: var(--lw-surface-raised);
    color: var(--lw-accent);
  }
  .dock-title, .dock-artist {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dock-title { font-size: 13.5px; font-weight: 600; line-height: 1.3; color: var(--lw-text-primary); }
  .dock-artist { font-size: 12px; color: var(--lw-text-secondary); }

  /* CENTER: Transport + Progress */
  .dock-center {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }
  .dock-transport-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }

  /* Plain glyph button matching _Glyph (34x34, no square background at rest) */
  .dock-glyph {
    display: inline-flex;
    width: 34px;
    height: 34px;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave), background-color var(--lw-motion-fast);
  }
  .dock-glyph.dock-large { color: var(--lw-text-primary); }
  .dock-glyph[aria-pressed="true"] { color: var(--lw-accent); }
  .dock-glyph:disabled { color: color-mix(in srgb, var(--lw-text-tertiary) 50%, transparent); cursor: default; }

  /* 36px accent disc matching _PlayGlyph */
  .dock-play {
    display: inline-flex;
    width: 36px;
    height: 36px;
    flex: none;
    align-items: center;
    justify-content: center;
    margin-inline: 2px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 50%;
    background: var(--lw-accent);
    color: var(--lw-on-accent);
    box-shadow: var(--lw-shadow-transport);
    cursor: pointer;
    transition: transform var(--lw-motion-fast) var(--ease-wave), background-color var(--lw-motion-fast);
  }
  .dock-play:disabled {
    background: color-mix(in srgb, var(--lw-accent) 35%, transparent);
    color: color-mix(in srgb, var(--lw-on-accent) 50%, transparent);
    cursor: default;
  }
  .dock-play-icons { display: grid; place-items: center; }
  .dock-play-icons > span {
    grid-area: 1 / 1;
    transition: opacity 180ms var(--ease-wave), scale 180ms var(--ease-wave);
  }
  .dock-play-icons[data-playing="false"] > span:last-child,
  .dock-play-icons[data-playing="true"] > span:first-child {
    opacity: 0;
    scale: 0.85;
  }
  .dock-spinner {
    width: 15px;
    height: 15px;
    border: 2px solid color-mix(in srgb, var(--lw-on-accent) 30%, transparent);
    border-top-color: var(--lw-on-accent);
    border-radius: 50%;
    animation: dock-spin 0.8s linear infinite;
  }
  @keyframes dock-spin { to { transform: rotate(360deg); } }

  .dock-press:not(:disabled):active { transform: scale(0.92); }

  /* Progress row */
  .dock-progress {
    display: flex;
    width: 100%;
    max-width: 560px;
    align-items: center;
    gap: 8px;
  }
  .dock-time {
    width: 38px;
    flex: none;
    font-size: 11px;
    color: var(--lw-text-tertiary);
  }
  .dock-time-pos { text-align: right; }
  .dock-time-dur { text-align: left; }

  /* Custom pointer-tracked slider (no native <input type="range">) */
  .dock-slider {
    position: relative;
    display: flex;
    height: 18px;
    min-width: 0;
    flex: 1;
    align-items: center;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .dock-slider.disabled { cursor: default; opacity: 0.5; }
  .dock-slider-track {
    position: relative;
    width: 100%;
    height: 3px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--lw-outline);
    transition: height 120ms ease;
  }
  .dock-slider.active .dock-slider-track { height: 4px; }
  .dock-slider-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--lw-accent);
  }
  .dock-wavy-svg {
    width: 100%;
    height: 12px;
    overflow: visible;
  }
  .dock-slider-thumb {
    position: absolute;
    top: 50%;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--lw-accent);
    transform: translate(-50%, -50%) scale(0);
    transition: transform 120ms ease;
    pointer-events: none;
  }
  .dock-slider.active .dock-slider-thumb,
  .dock-slider:focus-visible .dock-slider-thumb {
    transform: translate(-50%, -50%) scale(1);
  }

  .dock-seek-tooltip {
    position: absolute;
    bottom: 20px;
    padding: 2px 6px;
    border: 1px solid var(--lw-outline);
    border-radius: 5px;
    background: var(--lw-surface-raised);
    color: var(--lw-text-primary);
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    transform: translateX(-50%);
    pointer-events: none;
    box-shadow: var(--lw-shadow-flyout);
  }

  /* RIGHT */
  .dock-right {
    display: flex;
    min-width: 260px;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 1px;
  }

  .dock-quality-pill {
    padding: 2px 7px;
    margin-right: 4px;
    border: 1px solid var(--lw-outline);
    border-radius: 999px;
    background: var(--lw-surface);
    color: var(--lw-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.7px;
    cursor: pointer;
    transition: background-color var(--lw-motion-fast), border-color var(--lw-motion-fast);
  }
  .dock-quality-pill:hover {
    background: var(--lw-accent-subtle);
    border-color: var(--lw-accent);
  }

  .dock-volume {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-right: 4px;
  }
  .dock-vol-slider {
    width: 84px;
    flex: none;
  }

  @media (hover: hover) and (pointer: fine) {
    .dock-glyph:not(:disabled):hover {
      color: var(--lw-text-primary);
      transform: scale(1.08);
    }
    .dock-glyph[aria-pressed="true"]:hover { color: var(--lw-accent); }
    .dock-play:not(:disabled):hover { transform: scale(1.06); }
  }

  @media (max-width: 1040px) {
    .dock-left { width: 200px; }
    .dock-track-menu, .dock-quality-wrap { display: none; }
    .dock-right { min-width: 170px; }
    .dock-vol-slider { width: 56px; }
  }
</style>
