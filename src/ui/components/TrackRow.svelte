<script lang="ts">
  import { playTrackNow, toggleTrackLike } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import type { Track } from '$src/state/types';
  import { goAlbum } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import ArtistLine from './ArtistLine.svelte';
  import Artwork from './Artwork.svelte';
  import { contextMenu } from './contextMenuState.svelte';

  interface Props {
    track: Track;
    index?: number;
    list?: Track[];
    showAlbum?: boolean;
    showQuality?: boolean;
  }

  let { track, index, list, showAlbum = true, showQuality = true }: Props = $props();

  let isCurrent = $derived(playerStore.currentTrack?.id === track.id);
  let isPlayingCurrent = $derived(isCurrent && playerStore.isPlaying);
  let liked = $derived(libraryStore.isLiked(track.id));
</script>

<div
  role="row"
  tabindex="0"
  data-lw="track-row"
  data-active={isCurrent}
  onclick={() => playTrackNow(track, list)}
  onkeydown={(e) => e.key === 'Enter' && playTrackNow(track, list)}
  oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
  class="group flex h-[54px] cursor-pointer items-center rounded-[12px] px-2 transition-colors duration-[110ms] hover:bg-[var(--lw-wash-hover)]"
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

  <Artwork src={track.artworkUrl} alt={track.title} size={40} radius={12} />

  <div class="ml-3 min-w-0 flex-[5]">
    <div class="flex items-center gap-1.5">
      <span class="truncate text-[13.5px] font-semibold leading-[1.3] {isCurrent ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]'}">
        {track.title}
      </span>
      {#if track.isExplicit}
        <span class="rounded-[4px] bg-[var(--lw-surface-overlay)] px-1 text-[9.5px] font-bold text-[var(--lw-text-secondary)]">E</span>
      {/if}
    </div>
    <div class="flex items-center">
      <ArtistLine artists={track.artists} />
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
    <div class="ml-2 hidden shrink-0 items-center lg:flex">
      <span class="rounded-[6px] border border-[var(--lw-outline)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-tertiary)]">
        {track.codec}
      </span>
    </div>
  {/if}

  <div class="ml-2 flex shrink-0 items-center gap-0.5">
    <button
      type="button"
      onclick={(e) => {
        e.stopPropagation();
        toggleTrackLike(track);
      }}
      aria-label={liked ? 'Unlike track' : 'Like track'}
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] transition-all duration-[110ms] hover:bg-[var(--lw-wash-hover)] active:scale-95 {liked
        ? 'text-[var(--lw-danger)] opacity-100'
        : 'text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 hover:text-[var(--lw-text-primary)]'}"
    >
      <WaveIcon name="heart" size={15} filled={liked} />
    </button>

    <button
      type="button"
      onclick={(e) => contextMenu.showForTrack(e, track)}
      aria-label="Track options"
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-tertiary)] opacity-0 transition-all duration-[110ms] group-hover:opacity-100 hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="more" size={15} />
    </button>

    <span class="w-11 text-right text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">
      {formatDuration(track.durationSec)}
    </span>
  </div>
</div>
