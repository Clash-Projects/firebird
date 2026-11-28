<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { enqueueTrack, playTrackNow } from '$src/app/trackActions';
  import type { Track } from '$src/state/types';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import TrackRow from './TrackRow.svelte';

  type SortCol = 'none' | 'title' | 'album' | 'duration';
  type SortDir = 'asc' | 'desc';

  interface Props {
    tracks: Track[];
    showAlbum?: boolean;
    showQuality?: boolean;
    maxHeight?: string;
    removeLabel?: string;
    onRemove?: (track: Track, index: number) => void;
  }

  let {
    tracks,
    showAlbum = true,
    showQuality = false,
    maxHeight,
    removeLabel,
    onRemove,
  }: Props = $props();

  let tableEl = $state<HTMLDivElement | null>(null);
  let scrollEl = $state<HTMLDivElement | null>(null);
  let sortCol = $state<SortCol>('none');
  let sortDir = $state<SortDir>('asc');
  let focusedIndex = $state(-1);
  let anchorIndex = $state(-1);
  let selectedKeys = $state<Set<string>>(new Set());

  function trackKey(t: Track, idx: number): string {
    return `${t.id}::${idx}`;
  }

  let sortedEntries = $derived.by(() => {
    const mapped = tracks.map((track, originalIndex) => ({ track, originalIndex }));
    if (sortCol === 'none') return mapped;
    const mul = sortDir === 'asc' ? 1 : -1;
    return [...mapped].sort((a, b) => {
      if (sortCol === 'title') {
        return mul * a.track.title.localeCompare(b.track.title);
      }
      if (sortCol === 'album') {
        return mul * (a.track.album || '').localeCompare(b.track.album || '');
      }
      return mul * (a.track.durationSec - b.track.durationSec);
    });
  });

  let sortedTracks = $derived(sortedEntries.map((e) => e.track));
  let selectedTracks = $derived(
    sortedEntries
      .filter((e, i) => selectedKeys.has(trackKey(e.track, i)))
      .map((e) => e.track)
  );

  function cycleSort(col: Exclude<SortCol, 'none'>) {
    if (sortCol !== col) {
      sortCol = col;
      sortDir = 'asc';
    } else if (sortDir === 'asc') {
      sortDir = 'desc';
    } else {
      sortCol = 'none';
    }
    selectedKeys = new Set();
    focusedIndex = -1;
  }

  function handleRowClick(index: number, e: MouseEvent) {
    const key = trackKey(sortedEntries[index].track, index);
    if (e.shiftKey && anchorIndex >= 0) {
      const next = new Set(selectedKeys);
      const lo = Math.min(anchorIndex, index);
      const hi = Math.max(anchorIndex, index);
      for (let i = lo; i <= hi; i++) {
        next.add(trackKey(sortedEntries[i].track, i));
      }
      selectedKeys = next;
      focusedIndex = index;
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selectedKeys);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      selectedKeys = next;
      anchorIndex = index;
      focusedIndex = index;
      return;
    }
    selectedKeys = new Set();
    anchorIndex = index;
    focusedIndex = index;
    playTrackNow(sortedEntries[index].track, sortedTracks);
  }

  function handleTableKeydown(e: KeyboardEvent) {
    if (sortedEntries.length === 0) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const delta = e.key === 'ArrowDown' ? 1 : -1;
      const next =
        focusedIndex < 0
          ? 0
          : Math.max(0, Math.min(sortedEntries.length - 1, focusedIndex + delta));
      focusedIndex = next;
      if (virtualizer && useVirtual) {
        $virtualizer?.scrollToIndex(next, { align: 'auto' });
      } else {
        tableEl?.querySelector<HTMLElement>(`[data-row-index="${next}"]`)?.scrollIntoView({
          block: 'nearest',
        });
      }
    } else if (e.key === 'Enter' && focusedIndex >= 0 && focusedIndex < sortedEntries.length) {
      if (document.activeElement === tableEl) {
        e.preventDefault();
        playTrackNow(sortedEntries[focusedIndex].track, sortedTracks);
      }
    } else if (e.key === 'Delete' && onRemove && focusedIndex >= 0 && focusedIndex < sortedEntries.length) {
      e.preventDefault();
      const entry = sortedEntries[focusedIndex];
      onRemove(entry.track, entry.originalIndex);
    } else if (e.key === 'Escape' && selectedKeys.size > 0) {
      selectedKeys = new Set();
    }
  }

  let useVirtual = $derived(sortedEntries.length > 40 && Boolean(maxHeight));

  let virtualizer = $derived(
    useVirtual && scrollEl
      ? createVirtualizer<HTMLDivElement, HTMLDivElement>({
          count: sortedEntries.length,
          getScrollElement: () => scrollEl,
          estimateSize: () => 54,
          overscan: 8,
        })
      : null
  );
</script>

<div
  bind:this={tableEl}
  data-lw="track-table"
  role="grid"
  tabindex="0"
  onkeydown={handleTableKeydown}
  class="w-full focus:outline-none"
>
  {#if selectedTracks.length > 1}
    <div class="mb-2 flex items-center justify-between rounded-[8px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-overlay)] px-3 py-1.5 text-[12px]">
      <span class="font-medium text-[var(--lw-text-primary)]">{selectedTracks.length} tracks selected</span>
      <div class="flex items-center gap-2">
        <button
          type="button"
          onclick={() => playTrackNow(selectedTracks[0], selectedTracks)}
          class="cursor-pointer rounded-[6px] px-2 py-0.5 font-medium text-[var(--lw-accent)] hover:bg-[var(--lw-wash-hover)]"
        >
          Play selected
        </button>
        <button
          type="button"
          onclick={() => {
            selectedTracks.forEach((t) => enqueueTrack(t));
            selectedKeys = new Set();
          }}
          class="cursor-pointer rounded-[6px] px-2 py-0.5 text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          Add to queue
        </button>
        <button
          type="button"
          onclick={() => (selectedKeys = new Set())}
          class="cursor-pointer rounded-[6px] px-2 py-0.5 text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)]"
        >
          Clear
        </button>
      </div>
    </div>
  {/if}

  <div class="flex items-center border-b border-[var(--lw-divider)] px-2 pb-2 text-[10px] font-bold tracking-[0.9px] text-[var(--lw-text-tertiary)] uppercase">
    <span class="w-[30px] shrink-0 text-center">#</span>
    <span class="w-[40px] shrink-0"></span>
    <button
      type="button"
      onclick={() => cycleSort('title')}
      class="ml-3 flex min-w-0 flex-[5] cursor-pointer items-center gap-1 text-left transition-colors hover:text-[var(--lw-text-primary)] {sortCol === 'title' ? 'text-[var(--lw-accent)]' : ''}"
    >
      <span>Title</span>
      {#if sortCol === 'title'}
        <WaveIcon name={sortDir === 'asc' ? 'sortUp' : 'sortDown'} size={10} />
      {/if}
    </button>
    {#if showAlbum}
      <button
        type="button"
        onclick={() => cycleSort('album')}
        class="ml-3 hidden flex-[3] cursor-pointer items-center gap-1 text-left transition-colors hover:text-[var(--lw-text-primary)] md:flex {sortCol === 'album' ? 'text-[var(--lw-accent)]' : ''}"
      >
        <span>Album</span>
        {#if sortCol === 'album'}
          <WaveIcon name={sortDir === 'asc' ? 'sortUp' : 'sortDown'} size={10} />
        {/if}
      </button>
    {/if}
    {#if showQuality}
      <span class="ml-2 hidden w-16 shrink-0 lg:block">Format</span>
    {/if}
    <button
      type="button"
      onclick={() => cycleSort('duration')}
      class="ml-2 flex w-[118px] shrink-0 cursor-pointer items-center justify-end gap-1 text-right transition-colors hover:text-[var(--lw-text-primary)] {sortCol === 'duration' ? 'text-[var(--lw-accent)]' : ''}"
    >
      {#if sortCol === 'duration'}
        <WaveIcon name={sortDir === 'asc' ? 'sortUp' : 'sortDown'} size={10} />
      {/if}
      <span>Time</span>
    </button>
  </div>

  {#if useVirtual && virtualizer}
    {@const items = $virtualizer?.getVirtualItems() ?? []}
    {@const totalHeight = $virtualizer?.getTotalSize() ?? 0}
    <div bind:this={scrollEl} use:waveSmoothScroll class="lw-scroll relative mt-1" style:max-height={maxHeight}>
      <div class="relative w-full" style:height="{totalHeight}px">
        {#each items as row (row.key)}
          {@const entry = sortedEntries[row.index]}
          <div
            data-row-index={row.index}
            class="absolute top-0 left-0 w-full"
            style:transform="translateY({row.start}px)"
          >
            <TrackRow
              track={entry.track}
              index={row.index}
              list={sortedTracks}
              {showAlbum}
              {showQuality}
              selected={selectedKeys.has(trackKey(entry.track, row.index))}
              focused={focusedIndex === row.index}
              {removeLabel}
              onRemove={onRemove ? () => onRemove(entry.track, entry.originalIndex) : undefined}
              onRowClick={(e) => handleRowClick(row.index, e)}
            />
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <div bind:this={scrollEl} class="mt-1 space-y-0.5">
      {#each sortedEntries as entry, i (`${entry.track.id}-${i}`)}
        <div data-row-index={i}>
          <TrackRow
            track={entry.track}
            index={i}
            list={sortedTracks}
            {showAlbum}
            {showQuality}
            selected={selectedKeys.has(trackKey(entry.track, i))}
            focused={focusedIndex === i}
            {removeLabel}
            onRemove={onRemove ? () => onRemove(entry.track, entry.originalIndex) : undefined}
            onRowClick={(e) => handleRowClick(i, e)}
          />
        </div>
      {/each}
    </div>
  {/if}
</div>
