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
  let seekPreview = $state<number | null>(null);
  let position = $derived(seekPreview ?? playerStore.positionSec);
  let duration = $derived(playerStore.durationSec);
  let seekFill = $derived(duration > 0 ? Math.min(100, (position / duration) * 100) : 0);
  let volumeIcon = $derived<WaveIconName>(
    playerStore.muted || playerStore.volume === 0 ? 'volumeMute' : playerStore.volume < 0.5 ? 'volumeLow' : 'volumeHigh'
  );
  let quality = $derived(track ? (track.qualityTier === 'atmos' ? 'ATMOS' : track.codec) : '');

  const panels = [
    { id: 'lyrics', title: 'Lyrics (L)', icon: 'lyrics' },
    { id: 'queue', title: 'Queue (Q)', icon: 'queue' },
    { id: 'eq', title: 'Equalizer', icon: 'eq' },
  ] as const;

  function commitSeek(event: Event & { currentTarget: HTMLInputElement }) {
    playerStore.seek(event.currentTarget.valueAsNumber);
    seekPreview = null;
  }
</script>

<WaveHaze level="l1" dataLw="player-bar" class="player-dock">
  <div class="dock-track">
    {#if track}
      <button type="button" class="dock-cover dock-press" title="Open Now Playing" onclick={() => router.navigate('/now-playing')}>
        {#key track.id}
          <div class="lw-route-enter"><Artwork src={track.artworkUrl} alt={`Cover art for ${track.title}`} size={49} radius={9} /></div>
        {/key}
      </button>
      <button type="button" class="dock-track-info" onclick={() => router.navigate('/now-playing')}>
        <span class="dock-track-title" title={track.title}>{track.title}</span>
        <span class="dock-track-artist" title={track.artist}>{track.artist}</span>
      </button>
      <button
        type="button"
        class="dock-icon dock-press"
        aria-label={liked ? 'Unlike track' : 'Like track'}
        title={liked ? 'Unlike track' : 'Like track'}
        aria-pressed={liked}
        onclick={() => track && toggleTrackLike(track)}
      >
        <WaveIcon name="heart" size={17} filled={liked} />
      </button>
      <button
        type="button"
        class="dock-icon dock-track-menu"
        aria-label="Track options"
        title="Track options"
        onclick={(event) => track && contextMenu.showForTrack(event, track)}
      >
        <WaveIcon name="more" size={16} />
      </button>
    {:else}
      <span class="dock-placeholder" aria-hidden="true"><WaveIcon name="album" size={23} /></span>
      <div class="min-w-0">
        <div class="dock-track-title">Nothing playing</div>
        <div class="dock-track-artist">Pick a track to listen</div>
      </div>
    {/if}
  </div>

  <div data-lw="player-controls" class="dock-transport">
    <div class="dock-transport-buttons">
      <button type="button" class="dock-icon dock-press" onclick={() => playerStore.toggleShuffle()} title="Shuffle" aria-pressed={playerStore.shuffle}>
        <WaveIcon name="shuffle" size={16} />
      </button>
      <button type="button" class="dock-icon dock-skip dock-press" onclick={() => playerStore.previous()} title="Previous track" disabled={!track}>
        <WaveIcon name="prev" size={18} filled />
      </button>
      <button
        type="button"
        class="dock-play dock-press"
        onclick={() => playerStore.togglePlay()}
        title={playerStore.isPlaying ? 'Pause' : 'Play'}
        aria-label={playerStore.isPlaying ? 'Pause' : 'Play'}
        disabled={!track}
      >
        <span class="dock-play-icons" aria-hidden="true" data-playing={playerStore.isPlaying}>
          <span><WaveIcon name="play" size={18} filled /></span>
          <span><WaveIcon name="pause" size={18} filled /></span>
        </span>
      </button>
      <button type="button" class="dock-icon dock-skip dock-press" onclick={() => playerStore.next()} title="Next track" disabled={!track}>
        <WaveIcon name="next" size={18} filled />
      </button>
      <button type="button" class="dock-icon dock-press" onclick={() => playerStore.cycleRepeat()} title={`Repeat: ${playerStore.repeat}`} aria-pressed={playerStore.repeat !== 'off'}>
        <WaveIcon name={playerStore.repeat === 'one' ? 'repeatOne' : 'repeat'} size={16} />
      </button>
    </div>

    <div data-lw="player-seekbar" class="dock-progress lw-tabular">
      <span>{formatDuration(position)}</span>
      <input
        type="range"
        class="dock-range"
        aria-label="Seek track position"
        aria-valuetext={formatDuration(position)}
        min="0"
        max={duration || 1}
        step="1"
        value={position}
        style:--range-fill="{seekFill}%"
        disabled={!track || !duration}
        oninput={(event) => (seekPreview = event.currentTarget.valueAsNumber)}
        onchange={commitSeek}
        onpointercancel={() => (seekPreview = null)}
        onblur={() => (seekPreview = null)}
      />
      <span>{formatDuration(duration)}</span>
    </div>
  </div>

  <div class="dock-tools">
    {#each uiSlots.playerBarControls as control (control.id)}
      <button type="button" class="dock-icon dock-press" onclick={control.onClick} title={control.label}>
        <WaveIcon name={(control.icon as WaveIconName) || 'plugin'} size={16} />
      </button>
    {/each}
    {#each panels as panel (panel.id)}
      <button type="button" class="dock-icon dock-press" onclick={() => windowState.toggleRightPanel(panel.id)} title={panel.title} aria-pressed={windowState.activeRightPanel === panel.id}>
        <WaveIcon name={panel.icon} size={17} />
      </button>
    {/each}
    <button type="button" class="dock-icon dock-output dock-press" onclick={() => windowState.toggleRightPanel('output')} title="Signal path & Bit-Perfect output" aria-pressed={windowState.activeRightPanel === 'output'}>
      <WaveIcon name="speaker" size={17} />
      {#if quality}<span>{quality}</span>{/if}
    </button>
    <div class="dock-volume">
      <button type="button" class="dock-icon dock-press" onclick={() => playerStore.toggleMute()} title={playerStore.muted ? 'Unmute (M)' : 'Mute (M)'} aria-pressed={playerStore.muted}>
        <WaveIcon name={volumeIcon} size={17} />
      </button>
      <input type="range" class="dock-range" aria-label="Volume" aria-valuetext={`${Math.round(playerStore.volume * 100)}%`} min="0" max="100" step="1" value={Math.round(playerStore.volume * 100)} style:--range-fill="{playerStore.volume * 100}%" oninput={(event) => playerStore.setVolume(event.currentTarget.valueAsNumber / 100)} />
    </div>
    <button type="button" class="dock-icon dock-press" onclick={() => router.navigate('/now-playing')} title="Now Playing" aria-pressed={router.matches('/now-playing')}>
      <WaveIcon name="maximize" size={16} />
    </button>
    <button type="button" class="dock-icon dock-press" onclick={() => windowState.toggleMiniPlayer()} title="Mini player">
      <WaveIcon name="minimize" size={16} />
    </button>
  </div>
</WaveHaze>

<style>
  :global([data-lw="player-bar"].player-dock) {
    position: relative;
    z-index: 20;
    display: grid;
    height: var(--lw-dock-h);
    flex: none;
    grid-template-columns: minmax(162px, 1fr) minmax(220px, 520px) minmax(max-content, 1fr);
    align-items: center;
    gap: clamp(12px, 2vw, 29px);
    padding-inline: 15px;
    border-top: 1px solid var(--lw-outline);
    background: var(--lw-dock-translucent);
  }

  .dock-track, .dock-tools { display: flex; min-width: 0; align-items: center; gap: 5px; }
  .dock-tools { justify-content: flex-end; gap: 2px; }
  .dock-track { gap: 8px; }
  .dock-cover { flex: none; cursor: pointer; }
  .dock-track-info { min-width: 0; margin-right: 2px; cursor: pointer; text-align: left; }
  .dock-placeholder { display: grid; width: 49px; height: 49px; flex: none; place-items: center; border: 1px solid var(--lw-outline); border-radius: 9px; background: var(--lw-surface-raised); color: var(--lw-accent); }
  .dock-track-title, .dock-track-artist { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dock-track-title { font-size: 13.5px; font-weight: 700; line-height: 1.35; }
  .dock-track-artist { margin-top: 2px; color: var(--lw-text-secondary); font-size: 11.5px; }

  .dock-transport { display: grid; min-width: 0; gap: 3px; }
  .dock-transport-buttons { display: flex; align-items: center; justify-content: center; gap: 6px; }
  .dock-icon, .dock-play {
    display: inline-flex;
    width: 31px;
    height: 32px;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }
  .dock-skip { color: var(--lw-text-primary); }
  .dock-icon[aria-pressed="true"] { background: color-mix(in srgb, var(--lw-accent) 11%, transparent); color: var(--lw-accent); }
  .dock-icon:disabled { color: var(--lw-text-tertiary); cursor: default; }
  .dock-play { width: 39px; height: 39px; border-radius: 50%; background: var(--lw-accent); color: var(--lw-on-accent); }
  .dock-play-icons { display: grid; place-items: center; }
  .dock-play-icons > span {
    grid-area: 1 / 1;
    transition: opacity var(--lw-motion-fast) var(--ease-wave), scale var(--lw-motion-fast) var(--ease-wave);
  }
  .dock-play-icons[data-playing="false"] > span:last-child,
  .dock-play-icons[data-playing="true"] > span:first-child { opacity: 0; scale: calc(1 - 0.09 * var(--lw-motion-factor)); }
  .dock-play:disabled { background: var(--lw-surface-overlay); color: var(--lw-text-secondary); cursor: default; }
  .dock-press { transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave); }
  .dock-press:not(:disabled):active { transform: scale(calc(1 - 0.06 * var(--lw-motion-factor))); }
  .dock-output:has(span) { width: auto; gap: 4px; padding-inline: 6px; font-size: 10px; font-weight: 700; }
  .dock-volume { display: flex; align-items: center; gap: 2px; margin-right: 4px; }
  .dock-volume .dock-range { width: clamp(37px, 5.2vw, 82px); }
  .dock-progress { display: grid; grid-template-columns: 34px minmax(0, 1fr) 34px; align-items: center; gap: 7px; color: var(--lw-text-secondary); font-size: 10.5px; }
  .dock-progress span:first-child { text-align: right; }
  .dock-range {
    width: 100%;
    height: 16px;
    appearance: none;
    border-radius: 5px;
    background: transparent;
    cursor: pointer;
  }
  .dock-range:disabled { cursor: default; }
  .dock-range::-webkit-slider-runnable-track {
    height: 3px;
    border-radius: 4px;
    background: linear-gradient(90deg, var(--lw-accent) var(--range-fill), var(--lw-outline) var(--range-fill));
  }
  .dock-range::-webkit-slider-thumb {
    width: 9px;
    height: 9px;
    margin-top: -3px;
    appearance: none;
    border-radius: 50%;
    background: var(--lw-accent);
    opacity: 0;
    transition: opacity var(--lw-motion-fast);
  }
  .dock-range:not(:disabled):is(:hover, :focus-visible, :active)::-webkit-slider-thumb { opacity: 1; }

  @media (hover: hover) and (pointer: fine) {
    .dock-icon:not(:disabled):hover { background: var(--lw-wash-hover); color: var(--lw-text-primary); }
    .dock-play:not(:disabled):hover { background: color-mix(in srgb, var(--lw-accent) 88%, var(--lw-text-primary)); }
  }

  @media (max-width: 1050px) {
    .dock-track-menu, .dock-output span { display: none; }
    .dock-output:has(span) { width: 31px; padding: 0; }
  }
</style>
