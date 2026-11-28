<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let locked = $state(false);
  let dragFromIdx = $state<number | null>(null);
  let dragOverIdx = $state<number | null>(null);

  let current = $derived(playerStore.currentTrack);
  let upcoming = $derived(playerStore.queue.slice(playerStore.currentIndex + 1));

  function onDragStart(queueIdx: number, e: DragEvent) {
    if (locked) {
      e.preventDefault();
      return;
    }
    dragFromIdx = queueIdx;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', String(queueIdx));
    }
  }

  function onDragOver(queueIdx: number, e: DragEvent) {
    if (locked || dragFromIdx === null) return;
    e.preventDefault();
    dragOverIdx = queueIdx;
  }

  function onDrop(queueIdx: number, e: DragEvent) {
    e.preventDefault();
    if (!locked && dragFromIdx !== null && dragFromIdx !== queueIdx) {
      playerStore.moveQueueItem(dragFromIdx, queueIdx);
    }
    dragFromIdx = null;
    dragOverIdx = null;
  }

  function onDragEnd() {
    dragFromIdx = null;
    dragOverIdx = null;
  }
</script>

<WaveHaze
  level="l2"
  dataLw="queue-panel"
  class="flex h-full w-[min(360px,100vw)] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between px-4 pt-3.5 pb-1">
    <h2 class="text-[18px] font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]">Queue</h2>
    <button
      type="button"
      use:waveTooltip={'Close queue'}
      onclick={() => windowState.closeRightPanel()}
      aria-label="Close queue"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="close" size={12} />
    </button>
  </div>

  {#if current}
    <div class="mt-2 px-4">
      <div class="text-[9.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">NOW PLAYING</div>
      <div
        role="button"
        tabindex="0"
        oncontextmenu={(e) => contextMenu.showForTrack(e, current)}
        class="mt-1.5 flex h-[48px] items-center gap-2.5 rounded-[10px] bg-[var(--lw-accent-subtle)] px-2"
      >
        <Artwork src={current.artworkUrl} label={current.title} artist={current.artist} kind="track" size={36} radius={8} />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[13px] font-semibold text-[var(--lw-accent)]">{current.title}</div>
          <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{current.artist}</div>
        </div>
        {#if playerStore.isPlaying}
          <span class="lw-eq-bars mr-1" aria-hidden="true"><span></span><span></span><span></span></span>
        {:else}
          <WaveIcon name="pause" size={14} class="mr-1 text-[var(--lw-accent)]" />
        {/if}
      </div>
    </div>
    <div class="mx-4 my-2.5 h-px bg-[var(--lw-divider)]"></div>
  {/if}

  <div class="flex items-center px-4 pb-1">
    <span class="text-[9.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase lw-tabular">
      UP NEXT · {upcoming.length}
    </span>
    <div class="flex-1"></div>
    <button
      type="button"
      use:waveTooltip={locked ? 'Unlock queue order' : 'Lock queue order'}
      onclick={() => (locked = !locked)}
      class="cursor-pointer rounded-[6px] px-2 py-0.5 text-[11px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      {locked ? 'Unlock' : 'Lock'}
    </button>
    <button
      type="button"
      use:waveTooltip={'Clear upcoming tracks'}
      disabled={locked || upcoming.length === 0}
      onclick={() => playerStore.clearUpcoming()}
      class="ml-1.5 cursor-pointer rounded-[6px] px-2 py-0.5 text-[11px] font-semibold text-[var(--lw-accent)] hover:bg-[var(--lw-wash-hover)] disabled:cursor-default disabled:opacity-35"
    >
      Clear
    </button>
  </div>

  <div use:waveSmoothScroll class="lw-scroll flex-1 space-y-0.5 px-2 pb-3">
    {#if upcoming.length === 0}
      <div class="grid justify-items-center gap-2 px-5 py-12 text-center">
        <span class="grid h-10 w-10 place-items-center rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] text-[var(--lw-accent)]">
          <WaveIcon name="queue" size={18} />
        </span>
        <p class="text-[14px] font-semibold text-[var(--lw-text-primary)]">
          {current ? 'Queue ends here' : 'Your queue is empty'}
        </p>
        <p class="text-[12px] text-[var(--lw-text-secondary)]">Add a track to listen to it next.</p>
      </div>
    {:else}
      {#each upcoming as track, offset (track.id + '-' + offset)}
        {@const queueIdx = playerStore.currentIndex + 1 + offset}
        <div
          role="listitem"
          draggable={!locked}
          ondragstart={(e) => onDragStart(queueIdx, e)}
          ondragover={(e) => onDragOver(queueIdx, e)}
          ondrop={(e) => onDrop(queueIdx, e)}
          ondragend={onDragEnd}
          oncontextmenu={(e) =>
            contextMenu.showForTrack(e, track, {
              removeLabel: 'Remove from queue',
              onRemove: () => playerStore.removeFromQueue(queueIdx),
            })}
          class="group flex h-[48px] items-center gap-2 rounded-[10px] px-1.5 transition-colors hover:bg-[var(--lw-wash-hover)] {dragOverIdx === queueIdx
            ? 'border-t-2 border-[var(--lw-accent)]'
            : ''}"
        >
          {#if !locked}
            <span
              class="flex h-6 w-4 shrink-0 cursor-grab items-center justify-center text-[var(--lw-text-tertiary)] opacity-35 transition-opacity group-hover:opacity-85 active:cursor-grabbing"
              aria-hidden="true"
            >
              <WaveIcon name="gripperBar" size={12} />
            </span>
          {/if}

          <button
            type="button"
            onclick={() => playerStore.playTrack(track)}
            class="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left"
          >
            <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" size={36} radius={8} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-semibold text-[var(--lw-text-primary)]">{track.title}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
            </div>
          </button>

          <span class="text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
            {formatDuration(track.durationSec)}
          </span>

          {#if !locked}
            <button
              type="button"
              use:waveTooltip={'Remove from queue'}
              onclick={() => playerStore.removeFromQueue(queueIdx)}
              aria-label="Remove from queue"
              class="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-[var(--lw-wash-pressed)] hover:text-[var(--lw-danger)]"
            >
              <WaveIcon name="close" size={12} />
            </button>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</WaveHaze>
