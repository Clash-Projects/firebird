<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Offline Downloads" subtitle="Lossless tracks with embedded cover art & .lrc lyrics" />

  {#if libraryStore.downloads.length === 0}
    <States icon="download" title="No downloads yet" description="Save any track offline from its context menu." />
  {:else}
    <div class="space-y-2">
      {#each libraryStore.downloads as dl (dl.trackId)}
        {@const track = libraryStore.trackById(dl.trackId)}
        {#if track}
          <div class="flex items-center gap-3.5 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3">
            <Artwork src={track.artworkUrl} size={46} radius={10} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13.5px] font-semibold">{track.title}</div>
              <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
                {track.artist} · {dl.format} · {dl.sizeMb.toFixed(1)} MB
              </div>
              {#if dl.status === 'downloading'}
                <div class="mt-1.5 h-1.5 w-48 overflow-hidden rounded-full bg-[var(--lw-surface-overlay)]">
                  <div class="h-full bg-[var(--lw-accent)]" style:width="{dl.progress * 100}%"></div>
                </div>
              {:else}
                <div class="truncate text-[11px] text-[var(--lw-text-tertiary)]">{dl.localPath}</div>
              {/if}
            </div>
            <button
              type="button"
              onclick={() => playTrackNow(track)}
              class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)]"
              title="Play"
            >
              <WaveIcon name="play" size={15} filled />
            </button>
            <button
              type="button"
              onclick={() => libraryStore.removeDownload(dl.trackId)}
              class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
              title="Delete offline copy"
            >
              <WaveIcon name="trash" size={15} />
            </button>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>
