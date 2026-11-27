<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let locked = $state(false);
  let current = $derived(playerStore.currentTrack);
  let upcoming = $derived(playerStore.queue.slice(playerStore.currentIndex + 1));
</script>

<WaveHaze
  level="l2"
  dataLw="queue-panel"
  class="flex h-full w-[360px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between px-4 pt-3.5 pb-0.5">
    <h2 class="text-[18px] font-bold tracking-[-0.3px]">Queue</h2>
    <button
      type="button"
      onclick={() => windowState.closeRightPanel()}
      aria-label="Close queue"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  {#if current}
    <div class="mt-2.5 px-4">
      <div class="text-[9px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">NOW PLAYING</div>
      <div class="mt-1.5 flex h-[48px] items-center gap-2.5 rounded-[12px] bg-[var(--lw-accent)]/10 px-2">
        <Artwork src={current.artworkUrl} size={36} radius={8} />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[13px] font-semibold text-[var(--lw-accent)]">{current.title}</div>
          <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{current.artist}</div>
        </div>
        <span class="lw-eq-bars mr-1" aria-hidden="true"><span></span><span></span><span></span></span>
      </div>
    </div>
    <div class="mx-4 my-2.5 h-px bg-[var(--lw-divider)]"></div>
  {/if}

  <div class="flex items-center px-4 pb-1">
    <span class="text-[9px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase lw-tabular">
      UP NEXT · {upcoming.length}
    </span>
    <div class="flex-1"></div>
    <button
      type="button"
      onclick={() => (locked = !locked)}
      class="cursor-pointer rounded-[6px] px-1.5 py-0.5 text-[11px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      {locked ? 'Unlock' : 'Lock'}
    </button>
    <button
      type="button"
      disabled={locked || upcoming.length === 0}
      onclick={() => playerStore.clearUpcoming()}
      class="ml-2 cursor-pointer rounded-[6px] px-1.5 py-0.5 text-[11px] font-semibold text-[var(--lw-accent)] hover:bg-[var(--lw-wash-hover)] disabled:cursor-default disabled:opacity-35"
    >
      Clear
    </button>
  </div>

  <div class="lw-scroll flex-1 space-y-0.5 px-2 pb-3">
    {#if upcoming.length === 0}
      <div class="py-12 text-center text-[12px] text-[var(--lw-text-tertiary)]">Queue ends here.</div>
    {:else}
      {#each upcoming as track, offset (track.id + '-' + offset)}
        {@const queueIdx = playerStore.currentIndex + 1 + offset}
        <div class="group flex h-[48px] items-center gap-2.5 rounded-[10px] px-2 transition-colors hover:bg-[var(--lw-wash-hover)]">
          <button
            type="button"
            onclick={() => playerStore.playTrack(track)}
            class="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 text-left"
          >
            <Artwork src={track.artworkUrl} size={36} radius={8} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-semibold">{track.title}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
            </div>
          </button>

          <span class="text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
            {formatDuration(track.durationSec)}
          </span>

          {#if !locked}
            <button
              type="button"
              onclick={() => playerStore.removeFromQueue(queueIdx)}
              aria-label="Remove from queue"
              class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-tertiary)] opacity-0 group-hover:opacity-100 hover:bg-[var(--lw-wash-pressed)] hover:text-[var(--lw-danger)]"
            >
              <WaveIcon name="close" size={13} />
            </button>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</WaveHaze>
