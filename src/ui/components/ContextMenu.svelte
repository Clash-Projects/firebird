<script lang="ts">
  import {
    downloadTrack,
    enqueueTrack,
    playTrackNext,
    playTrackNow,
    toggleTrackLike,
  } from '$src/app/trackActions';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { goAlbum } from '$src/ui/navigation/waveNav';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { contextMenu } from './contextMenuState.svelte';

  let track = $derived(contextMenu.track);
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);
  let customPlaylists = $derived(libraryStore.playlists.filter((p) => p.id !== 'pl-liked' && p.source === 'local'));

  function run(fn: () => void) {
    fn();
    contextMenu.close();
  }
</script>

<svelte:window
  onclick={() => contextMenu.open && contextMenu.close()}
  onkeydown={(e) => e.key === 'Escape' && contextMenu.close()}
/>

{#if contextMenu.open && track}
  <div
    role="menu"
    tabindex="-1"
    class="fixed z-50"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.key === 'Escape' && contextMenu.close()}
  >
    <WaveHaze
      level="l3"
      dataLw="context-menu"
      class="w-[224px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 shadow-[var(--lw-shadow-flyout)]"
    >
      <div class="border-b border-[var(--lw-divider)] px-2.5 py-1.5">
        <div class="truncate text-[12.5px] font-semibold text-[var(--lw-text-primary)]">{track.title}</div>
        <div class="truncate text-[11px] text-[var(--lw-text-secondary)]">{track.artist}</div>
      </div>

      <div class="mt-1 space-y-0.5 text-[12.5px]">
        <button
          type="button"
          onclick={() => run(() => playTrackNow(track))}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="play" size={14} /> Play now
        </button>
        <button
          type="button"
          onclick={() => run(() => playTrackNext(track))}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="next" size={14} /> Play next
        </button>
        <button
          type="button"
          onclick={() => run(() => enqueueTrack(track))}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="addToQueue" size={14} /> Add to queue
        </button>
        <button
          type="button"
          onclick={() => run(() => toggleTrackLike(track))}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="heart" size={14} filled={liked} /> {liked ? 'Remove from Liked' : 'Save to Liked'}
        </button>
        <button
          type="button"
          onclick={() => run(() => downloadTrack(track))}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="download" size={14} /> Save offline
        </button>
        {#if track.albumId}
          <button
            type="button"
            onclick={() => run(() => goAlbum(track.albumId!))}
            class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="album" size={14} /> Go to album
          </button>
        {/if}

        {#if customPlaylists.length > 0}
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
          <div class="px-2.5 py-0.5 text-[10.5px] font-semibold tracking-wider text-[var(--lw-text-tertiary)] uppercase">
            Add to playlist
          </div>
          {#each customPlaylists.slice(0, 4) as pl (pl.id)}
            <button
              type="button"
              onclick={() =>
                run(() => {
                  libraryStore.addTrackToPlaylist(pl.id, track.id);
                  featuresStore.showToast({ title: `Added to ${pl.name}`, subtitle: track.title, severity: 'success' });
                })}
              class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left text-[12px] hover:bg-[var(--lw-wash-hover)]"
            >
              <WaveIcon name="plus" size={13} />
              <span class="truncate">{pl.name}</span>
            </button>
          {/each}
        {/if}

        {#if uiSlots.trackContextMenuItems.length > 0}
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
          {#each uiSlots.trackContextMenuItems as item (item.id)}
            <button
              type="button"
              onclick={() => run(() => item.onClick(track))}
              class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
            >
              <WaveIcon name="plugin" size={14} /> {item.label}
            </button>
          {/each}
        {/if}
      </div>
    </WaveHaze>
  </div>
{/if}
