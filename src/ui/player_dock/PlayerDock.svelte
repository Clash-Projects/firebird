<script lang="ts">
  import { clampSubmenu, dismissDialogOnBackdrop, dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { downloadTrack, toggleTrackLike } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import WaveSlider from '$src/ui/components/WaveSlider.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);
  let glass = $derived(settingsStore.glassmorphism);

  let seekDragging = $state(false);
  let seekDragSec = $state<number | null>(null);
  let seekHoverRatio = $state<number | null>(null);
  let seekBarEl = $state<HTMLDivElement | null>(null);

  let volDragging = $state(false);
  let volHover = $state(false);
  let volBarEl = $state<HTMLDivElement | null>(null);

  let qualityOpen = $state(false);
  let volumeFlyoutOpen = $state(false);
  let overflowOpen = $state(false);
  let dockSubmenu = $state<'output' | 'sleep' | null>(null);
  let dockSubHoverTimer: ReturnType<typeof setTimeout> | null = null;
  let dockSubCloseTimer: ReturnType<typeof setTimeout> | null = null;
  let sleepDialogOpen = $state(false);
  let sleepMinutesDraft = $state('45');
  let sleepDialogEl = $state<HTMLDialogElement | null>(null);

  function hoverDockSubmenu(key: 'output' | 'sleep') {
    if (dockSubCloseTimer) {
      clearTimeout(dockSubCloseTimer);
      dockSubCloseTimer = null;
    }
    if (dockSubmenu === key) return;
    if (dockSubHoverTimer) clearTimeout(dockSubHoverTimer);
    dockSubHoverTimer = setTimeout(() => {
      dockSubmenu = key;
    }, dockSubmenu ? 260 : 160);
  }

  function keepDockSubmenu() {
    if (dockSubHoverTimer) {
      clearTimeout(dockSubHoverTimer);
      dockSubHoverTimer = null;
    }
    if (dockSubCloseTimer) {
      clearTimeout(dockSubCloseTimer);
      dockSubCloseTimer = null;
    }
  }

  function scheduleCloseDockSubmenu() {
    if (dockSubHoverTimer) {
      clearTimeout(dockSubHoverTimer);
      dockSubHoverTimer = null;
    }
    if (!dockSubmenu) return;
    if (dockSubCloseTimer) clearTimeout(dockSubCloseTimer);
    dockSubCloseTimer = setTimeout(() => {
      dockSubmenu = null;
    }, 320);
  }

  $effect(() => {
    if (!sleepDialogEl) return;
    if (sleepDialogOpen && !sleepDialogEl.open) sleepDialogEl.showModal();
    else if (!sleepDialogOpen && sleepDialogEl.open) sleepDialogEl.close();
  });

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

  let speedLabel = $derived(
    Number.isInteger(playerStore.speed) ? `${playerStore.speed.toFixed(0)}×` : `${playerStore.speed}×`
  );

  // Wavy seekbar SVG path along played portion
  let wavyPath = $derived.by(() => {
    if (!settingsStore.wavySeekbar || seekPct <= 0.5) return '';
    const w = 200;
    const endX = (seekPct / 100) * w;
    const amp = 2.1;
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
    (e.currentTarget as HTMLElement)?.setPointerCapture(e.pointerId);
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

  function saveCustomSleep() {
    const mins = Number.parseInt(sleepMinutesDraft.trim(), 10);
    if (!Number.isFinite(mins) || mins < 1 || mins > 1440) return;
    playerStore.setSleepTimer(mins);
    sleepDialogOpen = false;
  }
</script>

<WaveHaze
  level={glass ? 'l3' : 'l1'}
  enableBlur={glass}
  dataLw="player-bar"
  class="player-dock {glass ? 'glass-sheen' : ''}"
>
  <!-- LEFT: Track Identity (290px matching player_dock.dart, 48px artwork radius 12) -->
  <div class="dock-left">
    {#if track}
      <button
        type="button"
        class="dock-cover dock-press"
        use:waveTooltip={'Open Now Playing'}
        aria-label="Open Now Playing"
        onclick={() => router.navigate('/now-playing')}
      >
        {#key track.id}
          <div class="lw-route-enter">
            <Artwork src={track.artworkUrl} label={track.title} alt={`Cover art for ${track.title}`} size={48} radius={12} />
          </div>
        {/key}
      </button>

      <button
        type="button"
        class="dock-identity"
        onclick={() => router.navigate('/now-playing')}
      >
        <span class="dock-title">{track.title}</span>
        <span class="dock-artist">{track.artist}</span>
      </button>

      <button
        type="button"
        class="dock-glyph dock-press"
        aria-label={liked ? 'Unlike' : 'Like'}
        use:waveTooltip={liked ? 'Unlike' : 'Like'}
        aria-pressed={liked}
        onclick={() => track && toggleTrackLike(track)}
      >
        <WaveIcon name="heart" size={15} filled={liked} />
      </button>

      <button
        type="button"
        class="dock-glyph dock-track-menu dock-press"
        aria-label="More track options"
        use:waveTooltip={'More'}
        onclick={(event) => track && contextMenu.showForTrack(event, track)}
      >
        <WaveIcon name="more" size={15} />
      </button>
    {:else}
      <div class="dock-idle-art" aria-hidden="true">
        <WaveIcon name="music" size={20} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="dock-title">Nothing playing</div>
        <div class="dock-artist text-[var(--lw-text-tertiary)]">Pick something to start</div>
      </div>
    {/if}
  </div>

  <!-- CENTER: Transport + Neutral 3px Fluent Seekbar -->
  <div data-lw="player-controls" class="dock-center">
    <div class="dock-transport-row">
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.toggleShuffle()}
        use:waveTooltip={'Shuffle'}
        aria-label="Shuffle"
        aria-pressed={playerStore.shuffle}
      >
        <WaveIcon name="shuffle" size={15} />
      </button>

      <button
        type="button"
        class="dock-glyph dock-large dock-press"
        onclick={() => playerStore.previous()}
        use:waveTooltip={'Previous'}
        aria-label="Previous"
        disabled={!track}
      >
        <WaveIcon name="prev" size={18} filled />
      </button>

      <button
        type="button"
        class="dock-play dock-press"
        onclick={() => playerStore.togglePlay()}
        use:waveTooltip={playerStore.isPlaying ? 'Pause (Space)' : 'Play (Space)'}
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
        use:waveTooltip={'Next'}
        aria-label="Next"
        disabled={!track}
      >
        <WaveIcon name="next" size={18} filled />
      </button>

      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.cycleRepeat()}
        use:waveTooltip={`Repeat ${playerStore.repeat}`}
        aria-label={`Repeat ${playerStore.repeat}`}
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
            <line x1="0" y1="6" x2="200" y2="6" stroke="var(--lw-wash-pressed)" stroke-width="2.6" stroke-linecap="round" />
            <path d={wavyPath} fill="none" stroke="var(--lw-text-primary)" stroke-width="2.6" stroke-linecap="round" />
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

      <span class="dock-time dock-time-dur">{duration > 0 ? formatDuration(duration) : '--:--'}</span>
    </div>
  </div>

  <!-- RIGHT: Rectangular quality badge, Panels, Volume, Expand, Mini, Overflow -->
  <div class="dock-right">
    {#each uiSlots.playerBarControls as control (control.id)}
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={control.onClick}
        use:waveTooltip={control.label}
        aria-label={control.label}
      >
        <WaveIcon name={(control.icon as WaveIconName) || 'plugin'} size={15} />
      </button>
    {/each}

    {#if track && qualityBadge}
      <div class="relative dock-quality-wrap" use:dismissFlyout={{ open: qualityOpen, close: () => (qualityOpen = false) }}>
        <button
          type="button"
          class="dock-quality-badge dock-press"
          aria-expanded={qualityOpen}
          use:waveTooltip={`${track.codec}${track.bitrateKbps > 0 ? ` · ${track.bitrateKbps} kbps` : ''} — details`}
          onclick={() => (qualityOpen = !qualityOpen)}
        >
          {qualityBadge}
        </button>

        <div
          data-open={qualityOpen}
          inert={!qualityOpen}
          class="lw-popover lw-flyout-surface absolute right-0 bottom-10 z-40 w-[264px]"
          style:--lw-popover-origin="bottom right"
        >
          <div class="lw-menu-row pointer-events-none">
            <WaveIcon name="activity" size={15} />
            <span class="min-w-0 flex-1 truncate">{qualityBadge} · {track.codec}</span>
          </div>
          <div class="lw-menu-sep"></div>
          <div class="lw-menu-row pointer-events-none">
            <WaveIcon name="music" size={15} />
            <span class="min-w-0 flex-1 truncate lw-tabular">
              Sample rate · {(track.sampleRateHz / 1000).toFixed(track.sampleRateHz % 1000 === 0 ? 0 : 1)} kHz{track.codec.toLowerCase().includes('opus') ? ' (Opus)' : ''}
            </span>
          </div>
          <div class="lw-menu-row pointer-events-none">
            <WaveIcon name="speaker" size={15} />
            <span class="min-w-0 flex-1 truncate">
              Output · {featuresStore.selectedDevice?.name ?? 'System default'}
            </span>
          </div>
          <div class="lw-menu-row pointer-events-none">
            <WaveIcon name="layers" size={15} />
            <span class="min-w-0 flex-1 truncate lw-tabular">
              {track.qualityTier === 'atmos' ? 'Spatial Audio · Dolby Atmos' : `Bit depth · ${track.bitDepth}-bit`}
            </span>
          </div>
          {#if track.bitrateKbps > 0}
            <div class="lw-menu-row pointer-events-none">
              <WaveIcon name="clock" size={15} />
              <span class="min-w-0 flex-1 truncate lw-tabular">Bitrate · {track.bitrateKbps} kbps</span>
            </div>
          {/if}
          <div class="lw-menu-sep"></div>
          <button
            type="button"
            onclick={() => {
              qualityOpen = false;
              router.navigate('/settings/audio');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="settings" size={15} />
            <span class="min-w-0 flex-1 truncate">Quality settings</span>
          </button>
        </div>
      </div>
    {/if}

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('queue')}
      use:waveTooltip={'Queue (Q)'}
      aria-label="Queue (Q)"
      aria-pressed={windowState.activeRightPanel === 'queue'}
    >
      <WaveIcon name="queue" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-press"
      onclick={() => windowState.toggleRightPanel('eq')}
      use:waveTooltip={'Equalizer'}
      aria-label="Equalizer"
      aria-pressed={windowState.activeRightPanel === 'eq'}
    >
      <WaveIcon name="eq" size={15} />
    </button>

    <!-- Wide inline Volume -->
    <div class="dock-volume-inline">
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => playerStore.toggleMute()}
        use:waveTooltip={volRatio === 0 ? 'Unmute (M)' : `Mute (${volPct}%)`}
        aria-label={volRatio === 0 ? 'Unmute' : 'Mute'}
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
        class="dock-slider dock-vol-slider"
        class:active={volDragging || volHover}
        class:has-val={volPct > 0}
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

    <!-- Compact Volume Flyout (< 820px) -->
    <div class="dock-volume-compact relative" use:dismissFlyout={{ open: volumeFlyoutOpen, close: () => (volumeFlyoutOpen = false) }}>
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => (volumeFlyoutOpen = !volumeFlyoutOpen)}
        use:waveTooltip={`Volume (${volPct}%)`}
        aria-label="Volume"
        aria-expanded={volumeFlyoutOpen}
      >
        <WaveIcon name={volumeIcon} size={15} />
      </button>

      <div
        data-open={volumeFlyoutOpen}
        inert={!volumeFlyoutOpen}
        class="lw-popover lw-flyout-surface absolute right-0 bottom-10 z-40 w-[224px] p-3 text-[12.5px]"
        style:--lw-popover-origin="bottom right"
      >
        <div class="mb-2 flex items-center justify-between font-semibold">
          <span>Volume {volPct}%</span>
        </div>
        <WaveSlider
          min={0}
          max={1}
          step={0.01}
          label="Volume slider"
          valuetext={`${volPct}%`}
          value={volRatio}
          oninput={(v) => playerStore.setVolume(v)}
        />
        <button
          type="button"
          onclick={() => {
            playerStore.toggleMute();
            volumeFlyoutOpen = false;
          }}
          class="mt-2 cursor-pointer text-[12px] font-semibold text-[var(--lw-accent)] hover:underline"
        >
          {volRatio === 0 ? 'Unmute' : 'Mute'}
        </button>
      </div>
    </div>

    <button
      type="button"
      class="dock-glyph dock-expand-btn dock-press"
      onclick={() => router.navigate(router.path === '/now-playing' ? '/home' : '/now-playing')}
      use:waveTooltip={'Now Playing'}
      aria-label="Now Playing"
      aria-pressed={router.path === '/now-playing'}
    >
      <WaveIcon name="maximize" size={15} />
    </button>

    <button
      type="button"
      class="dock-glyph dock-mini-btn dock-press"
      onclick={() => windowState.toggleMiniPlayer()}
      use:waveTooltip={'Mini player'}
      aria-label="Mini player"
    >
      <WaveIcon name="minimize" size={15} />
    </button>

    <!-- Overflow menu (_DockOverflow: Output, Speed, Sleep timer, Download — matches player_dock.dart) -->
    <div
      class="relative"
      use:dismissFlyout={{
        open: overflowOpen,
        close: () => {
          overflowOpen = false;
          dockSubmenu = null;
        },
      }}
    >
      <button
        type="button"
        class="dock-glyph dock-press"
        onclick={() => {
          overflowOpen = !overflowOpen;
          if (!overflowOpen) dockSubmenu = null;
        }}
        use:waveTooltip={'More'}
        aria-label="More playback controls"
        aria-expanded={overflowOpen}
      >
        <WaveIcon name="more" size={15} />
      </button>

      <div
        data-open={overflowOpen}
        inert={!overflowOpen}
        style:--lw-popover-origin="bottom right"
        class="lw-popover lw-has-cascade lw-flyout-surface absolute right-0 bottom-10 z-40 w-[256px]"
      >
        <!-- Output cascading submenu -->
        <div
          role="presentation"
          class="relative"
          onpointerenter={() => hoverDockSubmenu('output')}
          onpointerleave={scheduleCloseDockSubmenu}
        >
          <button
            type="button"
            data-highlighted={dockSubmenu === 'output'}
            onclick={() => (dockSubmenu = dockSubmenu === 'output' ? null : 'output')}
            class="lw-menu-row"
          >
            <WaveIcon name="speaker" size={15} />
            <span class="min-w-0 flex-1 truncate">
              Output · {featuresStore.selectedDevice?.name ?? 'System default'}
            </span>
            <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
          </button>

          {#if dockSubmenu === 'output'}
            <div
              role="menu"
              tabindex="-1"
              use:clampSubmenu
              onpointerenter={keepDockSubmenu}
              onpointerleave={scheduleCloseDockSubmenu}
              class="absolute right-full bottom-[-4px] z-50 w-[264px] pr-1.5"
            >
              <div class="lw-flyout-surface max-h-[340px] overflow-y-auto">
                <button
                  type="button"
                  onclick={() => {
                    featuresStore.selectDevice('');
                    overflowOpen = false;
                    dockSubmenu = null;
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if !featuresStore.selectedDeviceId}
                      <WaveIcon name="check" size={13} />
                    {/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">System default</span>
                </button>
                {#each featuresStore.devices.slice(0, 8) as dev (dev.id)}
                  <button
                    type="button"
                    onclick={() => {
                      featuresStore.selectDevice(dev.id);
                      overflowOpen = false;
                      dockSubmenu = null;
                    }}
                    class="lw-menu-row"
                  >
                    <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                      {#if featuresStore.selectedDeviceId === dev.id}
                        <WaveIcon name="check" size={13} />
                      {/if}
                    </span>
                    <span class="min-w-0 flex-1 truncate">{dev.name}</span>
                  </button>
                {/each}
                <div class="lw-menu-sep"></div>
                <div class="lw-menu-row pointer-events-none">
                  <WaveIcon name="activity" size={15} />
                  <span class="min-w-0 flex-1 truncate">
                    {settingsStore.platform === 'windows'
                      ? settingsStore.exclusiveMode
                        ? 'WASAPI · Exclusive'
                        : 'WASAPI · Shared'
                      : 'System output'}
                  </span>
                </div>
                <button
                  type="button"
                  onclick={() => {
                    overflowOpen = false;
                    dockSubmenu = null;
                    router.navigate('/settings/audio');
                  }}
                  class="lw-menu-row"
                >
                  <WaveIcon name="settings" size={15} />
                  <span class="min-w-0 flex-1 truncate">Output settings</span>
                </button>
              </div>
            </div>
          {/if}
        </div>

        <button
          type="button"
          onpointerenter={scheduleCloseDockSubmenu}
          onclick={() => playerStore.cycleSpeed()}
          class="lw-menu-row"
        >
          <WaveIcon name="activity" size={15} />
          <span class="min-w-0 flex-1 truncate lw-tabular">Speed {speedLabel}</span>
        </button>

        <!-- Sleep timer cascading submenu -->
        <div
          role="presentation"
          class="relative"
          onpointerenter={() => hoverDockSubmenu('sleep')}
          onpointerleave={scheduleCloseDockSubmenu}
        >
          <button
            type="button"
            data-highlighted={dockSubmenu === 'sleep'}
            onclick={() => (dockSubmenu = dockSubmenu === 'sleep' ? null : 'sleep')}
            class="lw-menu-row"
          >
            <WaveIcon name="clock" size={15} />
            <span class="min-w-0 flex-1 truncate lw-tabular">
              {playerStore.sleepTimerMin ? `Sleep · ${playerStore.sleepTimerMin}m` : 'Sleep timer'}
            </span>
            <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
          </button>

          {#if dockSubmenu === 'sleep'}
            <div
              role="menu"
              tabindex="-1"
              use:clampSubmenu
              onpointerenter={keepDockSubmenu}
              onpointerleave={scheduleCloseDockSubmenu}
              class="absolute right-full bottom-[-4px] z-50 w-[196px] pr-1.5"
            >
              <div class="lw-flyout-surface">
                <button
                  type="button"
                  onclick={() => {
                    playerStore.setSleepTimer(null);
                    overflowOpen = false;
                    dockSubmenu = null;
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if playerStore.sleepTimerMin === null}
                      <WaveIcon name="check" size={13} />
                    {/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">Off</span>
                </button>
                {#each [15, 30, 60] as mins (mins)}
                  <button
                    type="button"
                    onclick={() => {
                      playerStore.setSleepTimer(mins);
                      overflowOpen = false;
                      dockSubmenu = null;
                    }}
                    class="lw-menu-row"
                  >
                    <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                      {#if playerStore.sleepTimerMin === mins}
                        <WaveIcon name="check" size={13} />
                      {/if}
                    </span>
                    <span class="min-w-0 flex-1 truncate">{mins} minutes</span>
                  </button>
                {/each}
                <button
                  type="button"
                  onclick={() => {
                    overflowOpen = false;
                    dockSubmenu = null;
                    sleepDialogOpen = true;
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[14px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if playerStore.sleepTimerMin !== null && ![15, 30, 60].includes(playerStore.sleepTimerMin)}
                      <WaveIcon name="check" size={13} />
                    {/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">Custom…</span>
                </button>
              </div>
            </div>
          {/if}
        </div>

        <button
          type="button"
          disabled={!track}
          onpointerenter={scheduleCloseDockSubmenu}
          onclick={() => {
            if (!track) return;
            overflowOpen = false;
            dockSubmenu = null;
            void downloadTrack(track);
          }}
          class="lw-menu-row"
        >
          <WaveIcon name="download" size={15} />
          <span class="min-w-0 flex-1 truncate">Download this track</span>
        </button>
      </div>
    </div>
  </div>
</WaveHaze>

<dialog
  bind:this={sleepDialogEl}
  use:dismissDialogOnBackdrop={() => (sleepDialogOpen = false)}
  onclose={() => (sleepDialogOpen = false)}
  closedby="any"
  aria-labelledby="sleep-dialog-title"
  class="w-full max-w-[360px]"
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      saveCustomSleep();
    }}
    class="space-y-4 p-5"
  >
    <h1 id="sleep-dialog-title" class="text-[16px] font-bold">Sleep timer</h1>
    <div class="space-y-1.5">
      <div class="text-[12.5px] font-semibold text-[var(--lw-text-secondary)]">Minutes (1–1440)</div>
      <Input bind:value={sleepMinutesDraft} autofocus placeholder="45" />
    </div>
    <div class="flex items-center justify-end gap-2">
      <Buttons variant="secondary" size="sm" onclick={() => (sleepDialogOpen = false)}>Cancel</Buttons>
      <Buttons variant="primary" size="sm" type="submit">Start</Buttons>
    </div>
  </form>
</dialog>

<style>
  :global([data-lw="player-bar"].player-dock) {
    position: relative;
    z-index: 20;
    display: flex;
    height: var(--lw-dock-h, 68px);
    flex: none;
    align-items: center;
    gap: 10px;
    padding-inline: 12px;
    border-top: 1px solid var(--lw-divider);
    background: var(--lw-dock-translucent);
  }

  :global([data-lw="player-bar"].player-dock.glass-sheen) {
    border-top-color: rgba(255, 255, 255, 0.16);
    background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.08) 0%, transparent 70%);
  }

  :global(:root[data-theme="light"] [data-lw="player-bar"].player-dock.glass-sheen) {
    border-top-color: rgba(0, 0, 0, 0.1);
    background-image: linear-gradient(180deg, rgba(255, 255, 255, 0.3) 0%, transparent 70%);
  }

  /* LEFT: 290px identity (matches player_dock.dart) */
  .dock-left {
    display: flex;
    width: 290px;
    min-width: 0;
    flex: none;
    align-items: center;
    gap: 10px;
  }
  .dock-cover {
    flex: none;
    cursor: pointer;
  }
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
    border-radius: 12px;
    background: var(--lw-surface-overlay);
    color: var(--lw-text-tertiary);
  }
  .dock-title,
  .dock-artist {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dock-title {
    font-size: 13.5px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--lw-text-primary);
  }
  .dock-artist {
    font-size: 12px;
    color: var(--lw-text-secondary);
  }

  /* CENTER: Transport + Neutral 3px Progress */
  .dock-center {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
  }
  .dock-transport-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }

  .dock-glyph {
    display: inline-flex;
    width: 34px;
    height: 34px;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 10px;
    color: color-mix(in srgb, var(--lw-text-primary) 78%, transparent);
    cursor: pointer;
    transition:
      color var(--lw-motion-fast) ease,
      transform var(--lw-motion-fast) var(--ease-wave),
      background-color var(--lw-motion-fast) ease;
  }
  .dock-glyph.dock-large {
    color: var(--lw-text-primary);
  }
  .dock-glyph[aria-pressed="true"] {
    color: var(--lw-accent);
  }
  .dock-glyph:disabled {
    color: color-mix(in srgb, var(--lw-text-tertiary) 50%, transparent);
    cursor: default;
  }

  .dock-play {
    display: inline-flex;
    width: 34px;
    height: 34px;
    flex: none;
    align-items: center;
    justify-content: center;
    margin-inline: 2px;
    border: none;
    border-radius: 50%;
    background: var(--lw-accent);
    color: var(--lw-on-accent);
    cursor: pointer;
    transition:
      transform var(--lw-motion-fast) var(--ease-wave),
      background-color var(--lw-motion-fast) ease;
  }
  .dock-play:disabled {
    background: color-mix(in srgb, var(--lw-accent) 35%, transparent);
    color: color-mix(in srgb, var(--lw-on-accent) 50%, transparent);
    cursor: default;
  }
  .dock-play-icons {
    display: grid;
    place-items: center;
  }
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
  @keyframes dock-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .dock-press:not(:disabled):active {
    transform: scale(0.92);
  }

  /* Progress & Volume — neutral primary ink (matches player_dock.dart white/black bars) */
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
    color: color-mix(in srgb, var(--lw-text-primary) 72%, transparent);
  }
  .dock-time-pos {
    text-align: right;
  }
  .dock-time-dur {
    text-align: left;
  }

  .dock-slider {
    position: relative;
    display: flex;
    height: 16px;
    min-width: 0;
    flex: 1;
    align-items: center;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .dock-slider.disabled {
    cursor: default;
    opacity: 0.45;
  }
  .dock-slider-track {
    position: relative;
    width: 100%;
    height: 3px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in srgb, var(--lw-text-primary) 13%, transparent);
  }
  .dock-slider-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--lw-text-primary);
  }
  .dock-wavy-svg {
    width: 100%;
    height: 12px;
    overflow: visible;
  }
  .dock-slider-thumb {
    position: absolute;
    top: 50%;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--lw-text-primary);
    transform: translate(-50%, -50%) scale(1);
    transition: transform 110ms var(--ease-wave), box-shadow 110ms ease;
    pointer-events: none;
  }
  .dock-slider.active .dock-slider-thumb,
  .dock-slider:focus-visible .dock-slider-thumb {
    transform: translate(-50%, -50%) scale(1.2);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--lw-accent) 24%, transparent);
  }
  .dock-vol-slider .dock-slider-thumb {
    width: 8px;
    height: 8px;
    opacity: 0;
  }
  .dock-vol-slider.has-val .dock-slider-thumb,
  .dock-vol-slider.active .dock-slider-thumb {
    opacity: 1;
  }

  .dock-seek-tooltip {
    position: absolute;
    bottom: 22px;
    padding: 2px 6px;
    border: 1px solid var(--lw-outline);
    border-radius: 6px;
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

  /* Rectangular overline quality badge matching _QualityFlyout in player_dock.dart */
  .dock-quality-badge {
    padding: 3.5px 7px;
    margin-right: 4px;
    border: 1px solid color-mix(in srgb, var(--lw-text-primary) 12%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--lw-text-primary) 6%, transparent);
    color: color-mix(in srgb, var(--lw-text-primary) 82%, transparent);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.8px;
    line-height: 1;
    cursor: pointer;
    transition: background-color var(--lw-motion-fast) ease, color var(--lw-motion-fast) ease;
  }
  .dock-quality-badge:hover {
    background: var(--lw-wash-hover);
    color: var(--lw-text-primary);
  }

  .dock-volume-inline {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-right: 2px;
  }
  .dock-vol-slider {
    width: 84px;
    flex: none;
  }
  .dock-volume-compact {
    display: none;
  }

  @media (hover: hover) and (pointer: fine) {
    .dock-glyph:not(:disabled):hover {
      color: var(--lw-text-primary);
      background: var(--lw-wash-hover);
    }
    .dock-glyph[aria-pressed="true"]:hover {
      color: var(--lw-accent);
    }
    .dock-play:not(:disabled):hover {
      transform: scale(1.06);
    }
  }

  @media (max-width: 960px) {
    .dock-left {
      width: 200px;
    }
    .dock-track-menu,
    .dock-quality-wrap {
      display: none;
    }
  }

  @media (max-width: 820px) {
    .dock-right {
      min-width: 150px;
    }
    .dock-volume-inline {
      display: none;
    }
    .dock-volume-compact {
      display: block;
    }
  }
</style>
