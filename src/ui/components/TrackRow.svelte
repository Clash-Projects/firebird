<script lang="ts">
  import { playTrackNow, toggleTrackLike } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import type { Track } from '$src/state/types';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { goAlbum } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import ArtistLine from './ArtistLine.svelte';
  import Artwork from './Artwork.svelte';
  import { contextMenu } from './contextMenuState.svelte';

  interface Props {
    track: Track;
    index?: number;
    list?: Track[];
    presentation?: 'standard' | 'artist';
    showAlbum?: boolean;
    showQuality?: boolean;
    selected?: boolean;
    focused?: boolean;
    removeLabel?: string;
    onRemove?: () => void;
    onRowClick?: (e: MouseEvent) => void;
  }

  let {
    track,
    index,
    list,
    presentation = 'standard',
    showAlbum = true,
    showQuality = true,
    selected = false,
    focused = false,
    removeLabel,
    onRemove,
    onRowClick,
  }: Props = $props();

  let isCurrent = $derived(playerStore.currentTrack?.id === track.id);
  let isPlayingCurrent = $derived(isCurrent && playerStore.isPlaying);
  let liked = $derived(libraryStore.isLiked(track.id));
  let artistRow = $derived(presentation === 'artist');
</script>

<div
  role="row"
  tabindex="0"
  data-lw="track-row"
  data-active={isCurrent}
  data-selected={selected}
  data-focused={focused}
  onclick={(e) => (onRowClick ? onRowClick(e) : playTrackNow(track, list))}
  onkeydown={(e) => e.target === e.currentTarget && e.key === 'Enter' && playTrackNow(track, list)}
  oncontextmenu={(e) =>
    contextMenu.showForTrack(e, track, removeLabel && onRemove ? { removeLabel, onRemove } : undefined)}
  class="track-row group flex {artistRow ? 'h-[var(--artist-track-height,54px)]' : 'h-[54px]'} cursor-pointer items-center rounded-[10px] px-2 transition-colors duration-[140ms] ease-[var(--ease-native)] hover:bg-[var(--lw-wash-hover)] {selected
    ? 'bg-[var(--lw-accent-subtle)]'
    : ''}"
>
  {#if index !== undefined}
    <div class="flex w-[30px] shrink-0 items-center justify-center text-[12px] lw-tabular">
      {#if isPlayingCurrent}
        <span class="lw-eq-bars group-hover:hidden" aria-hidden="true">
          <span></span><span></span><span></span>
        </span>
        <span class="hidden text-[var(--lw-text-primary)] group-hover:inline">
          <WaveIcon name="pause" size={14} filled />
        </span>
      {:else}
        <span class="group-hover:hidden {isCurrent ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">
          {String(index + 1).padStart(2, '0')}
        </span>
        <span class="hidden text-[var(--lw-text-primary)] group-hover:inline">
          <WaveIcon name="play" size={14} filled />
        </span>
      {/if}
    </div>
  {/if}

  <Artwork
    src={track.artworkUrl}
    label={track.title}
    artist={track.artist}
    album={track.album}
    kind="track"
    alt={artistRow ? '' : track.title}
    size={40}
    radius={artistRow ? 7 : 10}
  />

  <div class="ml-3 min-w-0 flex-[5]">
    <div class="flex items-center gap-1.5">
      <span class="truncate {artistRow ? 'text-[15px]' : 'text-[13.5px]'} font-semibold leading-[1.3] {isCurrent ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]'}">
        {track.title}
      </span>
      {#if track.isExplicit}
        <span class="rounded-[4px] bg-[var(--lw-surface-overlay)] px-1 text-[9.5px] font-bold text-[var(--lw-text-secondary)]">E</span>
      {/if}
    </div>
    <div class="flex items-center text-[12px] text-[var(--lw-text-secondary)]">
      {#if artistRow && track.album}
        {#if track.albumId}
          <button
            type="button"
            class="cursor-pointer truncate text-left hover:text-[var(--lw-text-primary)] hover:underline"
            onclick={(e) => {
              e.stopPropagation();
              if (track.albumId) goAlbum(track.albumId);
            }}
          >{track.album}</button>
        {:else}
          <span class="truncate">{track.album}</span>
        {/if}
      {:else}
        <ArtistLine artists={track.artists} />
      {/if}
    </div>
  </div>

  {#if showAlbum}
    <button
      type="button"
      onclick={(e) => {
        e.stopPropagation();
        if (track.albumId) goAlbum(track.albumId);
      }}
      class="ml-3 hidden flex-[3] cursor-pointer truncate text-left text-[12px] text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)] hover:underline md:block"
    >
      {track.album}
    </button>
  {/if}

  {#if showQuality}
    <div class="ml-2 hidden w-16 shrink-0 items-center lg:flex">
      <span class="rounded-[6px] border border-[var(--lw-outline)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-tertiary)]">
        {track.codec}
      </span>
    </div>
  {/if}

  <div class="ml-2 flex {artistRow ? 'gap-2' : 'w-[118px] gap-0.5'} shrink-0 items-center justify-end">
    {#if !artistRow}
      <button
        type="button"
        use:waveTooltip={liked ? 'Remove from Liked' : 'Like'}
        onclick={(e) => {
          e.stopPropagation();
          toggleTrackLike(track);
        }}
        aria-label={liked ? 'Unlike track' : 'Like track'}
        class="lw-press flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)] {liked
          ? 'text-[var(--lw-danger)] opacity-100'
          : 'text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-[var(--lw-text-primary)]'}"
      >
        <WaveIcon name="heart" size={14} filled={liked} />
      </button>
    {/if}

    <button
      type="button"
      use:waveTooltip={'More options'}
      onclick={(e) =>
        contextMenu.showForTrack(e, track, removeLabel && onRemove ? { removeLabel, onRemove } : undefined)}
      aria-label="Track options"
      class="{artistRow ? 'order-last text-[var(--lw-text-secondary)]' : 'text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 focus-visible:opacity-100'} flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] transition-[opacity,background-color,color] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="more" size={artistRow ? 16 : 14} />
    </button>

    <span class="{artistRow ? 'text-[var(--lw-text-secondary)]' : 'w-11 text-[var(--lw-text-tertiary)]'} text-right text-[12px] lw-tabular">
      {formatDuration(track.durationSec)}
    </span>
  </div>
</div>

<style>
  .track-row[data-focused='true'] {
    outline: 1.5px solid var(--lw-accent);
    outline-offset: -1.5px;
  }
</style>
