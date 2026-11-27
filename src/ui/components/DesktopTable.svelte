<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import type { Track } from '$src/state/types';
  import TrackRow from './TrackRow.svelte';

  interface Props {
    tracks: Track[];
    showAlbum?: boolean;
    showQuality?: boolean;
    maxHeight?: string;
  }

  let { tracks, showAlbum = true, showQuality = true, maxHeight }: Props = $props();
  let scrollEl = $state<HTMLDivElement | null>(null);

  let useVirtual = $derived(tracks.length > 40 && Boolean(maxHeight));

  let virtualizer = $derived(
    useVirtual && scrollEl
      ? createVirtualizer<HTMLDivElement, HTMLDivElement>({
          count: tracks.length,
          getScrollElement: () => scrollEl,
          estimateSize: () => 54,
          overscan: 8,
        })
      : null
  );
</script>

<div data-lw="track-table" class="w-full">
  <div class="flex items-center gap-3 border-b border-[var(--lw-divider)] px-2.5 pb-2 text-[11px] font-semibold tracking-[0.4px] text-[var(--lw-text-tertiary)] uppercase">
    <span class="w-6 text-center">#</span>
    <span class="flex-1">Title</span>
    {#if showAlbum}
      <span class="hidden w-[28%] md:block">Album</span>
    {/if}
    {#if showQuality}
      <span class="hidden w-16 lg:block">Format</span>
    {/if}
    <span class="w-24 text-right">Time</span>
  </div>

  {#if useVirtual && virtualizer}
    {@const items = $virtualizer?.getVirtualItems() ?? []}
    {@const totalHeight = $virtualizer?.getTotalSize() ?? 0}
    <div bind:this={scrollEl} class="lw-scroll relative mt-1" style:max-height={maxHeight}>
      <div class="relative w-full" style:height="{totalHeight}px">
        {#each items as row (row.key)}
          <div
            class="absolute top-0 left-0 w-full"
            style:transform="translateY({row.start}px)"
          >
            <TrackRow track={tracks[row.index]} index={row.index} list={tracks} {showAlbum} {showQuality} />
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <div bind:this={scrollEl} class="mt-1 space-y-0.5">
      {#each tracks as track, i (track.id + '-' + i)}
        <TrackRow {track} index={i} list={tracks} {showAlbum} {showQuality} />
      {/each}
    </div>
  {/if}
</div>
