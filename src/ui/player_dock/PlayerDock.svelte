<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { toggleTrackLike } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);
  let volumeIcon = $derived<WaveIconName>(
    playerStore.muted || playerStore.volume === 0
      ? 'volumeMute'
      : playerStore.volume < 0.5
        ? 'volumeLow'
        : 'volumeHigh'
  );

  let qualityBadge = $derived(
    track
      ? track.qualityTier === 'atmos'
        ? 'ATMOS'
        : `${track.codec} ${track.bitDepth}/${Math.round(track.sampleRateHz / 1000)}`
      : ''
  );

  function seekAtPointer(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    playerStore.seek(pct * playerStore.durationSec);
  }

  function volumeAtPointer(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    playerStore.setVolume(pct);
  }
</script>

<WaveHaze
  level="l1"
  dataLw="player-bar"
  class="flex h-[68px] shrink-0 items-center justify-between gap-3 border-t border-[var(--lw-divider)] bg-[var(--lw-dock-translucent)] px-3"
>
  <div class="flex w-[220px] shrink-0 items-center gap-2.5 lg:w-[290px]">
    {#if track}
      <button
        type="button"
        onclick={() => router.navigate('/now-playing')}
        class="shrink-0 cursor-pointer transition-transform active:scale-95"
        title="Open Now Playing"
      >
        <Artwork src={track.artworkUrl} alt={track.title} size={48} radius={12} shadow="lift" />
      </button>

      <button
        type="button"
        onclick={() => router.navigate('/now-playing')}
        class="min-w-0 flex-1 cursor-pointer text-left"
      >
        <div class="truncate text-[13.5px] font-semibold leading-[1.3] text-[var(--lw-text-primary)]">
          {track.title}
        </div>
        <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
          {track.artist}
        </div>
      </button>

      <button
        type="button"
        onclick={() => toggleTrackLike(track)}
        aria-label="Toggle like"
        class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-[10px] transition-colors hover:bg-[var(--lw-wash-hover)] {liked
          ? 'text-[var(--lw-danger)]'
          : 'text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]'}"
      >
        <WaveIcon name="heart" size={16} filled={liked} />
      </button>

      <button
        type="button"
        onclick={(e) => contextMenu.showForTrack(e, track)}
        aria-label="More options"
        class="hidden h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-tertiary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] sm:flex"
      >
        <WaveIcon name="more" size={15} />
      </button>
    {/if}
  </div>

  <div data-lw="player-controls" class="flex min-w-0 max-w-[560px] flex-1 flex-col items-center justify-center gap-0.5">
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        onclick={() => playerStore.toggleShuffle()}
        title="Shuffle"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] transition-colors hover:bg-[var(--lw-wash-hover)] {playerStore.shuffle
          ? 'text-[var(--lw-accent)]'
          : 'text-[var(--lw-text-secondary)]'}"
      >
        <WaveIcon name="shuffle" size={15} />
      </button>
      <button
        type="button"
        onclick={() => playerStore.previous()}
        title="Previous"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="prev" size={17} />
      </button>
      <button
        type="button"
        onclick={() => playerStore.togglePlay()}
        title={playerStore.isPlaying ? 'Pause' : 'Play'}
        class="mx-0.5 flex h-[34px] w-[34px] cursor-pointer items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[var(--lw-shadow-transport)] transition-transform active:scale-95"
      >
        <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={16} filled />
      </button>
      <button
        type="button"
        onclick={() => playerStore.next()}
        title="Next"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="next" size={17} />
      </button>
      <button
        type="button"
        onclick={() => playerStore.cycleRepeat()}
        title="Repeat"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] transition-colors hover:bg-[var(--lw-wash-hover)] {playerStore.repeat !== 'off'
          ? 'text-[var(--lw-accent)]'
          : 'text-[var(--lw-text-secondary)]'}"
      >
        <WaveIcon name={playerStore.repeat === 'one' ? 'repeatOne' : 'repeat'} size={15} />
      </button>
    </div>

    <div data-lw="player-seekbar" class="flex w-full items-center gap-2">
      <span class="w-8 text-right text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
        {formatDuration(playerStore.positionSec)}
      </span>
      <button
        type="button"
        onclick={seekAtPointer}
        aria-label="Seek track position"
        class="group relative flex h-4 flex-1 cursor-pointer items-center"
      >
        <span class="relative h-[3px] w-full overflow-hidden rounded-full bg-white/15 transition-[height] group-hover:h-[4px]">
          <span
            class="block h-full rounded-full bg-[var(--lw-accent)]"
            style:width="{playerStore.progress * 100}%"
          ></span>
        </span>
        <span
          class="pointer-events-none absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[var(--lw-accent)] opacity-0 shadow transition-opacity group-hover:opacity-100"
          style:left="{playerStore.progress * 100}%"
        ></span>
      </button>
      <span class="w-8 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
        {formatDuration(playerStore.durationSec)}
      </span>
    </div>
  </div>

  <div class="flex min-w-[200px] shrink-0 items-center justify-end gap-0.5 lg:min-w-[290px]">
    {#if qualityBadge}
      <button
        type="button"
        onclick={() => windowState.toggleRightPanel('output')}
        title="Signal path & Bit-Perfect output"
        class="mr-1 hidden cursor-pointer rounded-[6px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-2 py-0.5 text-[10px] font-bold tracking-[0.6px] text-[var(--lw-text-secondary)] transition-colors hover:border-[var(--lw-accent)] hover:text-[var(--lw-text-primary)] xl:inline-flex"
      >
        {qualityBadge}
      </button>
    {/if}

    {#each uiSlots.playerBarControls as ctrl (ctrl.id)}
      <button
        type="button"
        onclick={ctrl.onClick}
        title={ctrl.label}
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name={(ctrl.icon as WaveIconName) || 'plugin'} size={15} />
      </button>
    {/each}

    <button
      type="button"
      onclick={() => windowState.toggleRightPanel('queue')}
      title="Queue (Q)"
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] hover:bg-[var(--lw-wash-hover)] {windowState.activeRightPanel === 'queue'
        ? 'text-[var(--lw-accent)]'
        : 'text-[var(--lw-text-secondary)]'}"
    >
      <WaveIcon name="queue" size={16} />
    </button>

    <button
      type="button"
      onclick={() => windowState.toggleRightPanel('eq')}
      title="Equalizer"
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] hover:bg-[var(--lw-wash-hover)] {windowState.activeRightPanel === 'eq'
        ? 'text-[var(--lw-accent)]'
        : 'text-[var(--lw-text-secondary)]'}"
    >
      <WaveIcon name="eq" size={16} />
    </button>

    <div class="hidden items-center gap-1 md:flex">
      <button
        type="button"
        onclick={() => playerStore.toggleMute()}
        title="Mute (M)"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name={volumeIcon} size={16} />
      </button>
      <button
        type="button"
        onclick={volumeAtPointer}
        aria-label="Volume"
        class="group relative flex h-4 w-[84px] cursor-pointer items-center"
      >
        <span class="relative h-[3px] w-full overflow-hidden rounded-full bg-white/15">
          <span
            class="block h-full rounded-full bg-[var(--lw-accent)]"
            style:width="{playerStore.volume * 100}%"
          ></span>
        </span>
        <span
          class="pointer-events-none absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[var(--lw-accent)] opacity-0 transition-opacity group-hover:opacity-100"
          style:left="{playerStore.volume * 100}%"
        ></span>
      </button>
    </div>

    <button
      type="button"
      onclick={() => router.navigate('/now-playing')}
      title="Now Playing"
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="maximize" size={15} />
    </button>

    <button
      type="button"
      onclick={() => windowState.toggleMiniPlayer()}
      title="Mini player"
      class="hidden h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] lg:flex"
    >
      <WaveIcon name="minimize" size={15} />
    </button>
  </div>
</WaveHaze>
