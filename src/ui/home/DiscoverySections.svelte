<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import { goAlbum, goGenre } from '$src/ui/navigation/waveNav';
</script>

<div class="space-y-8">
  <Shelf title="New & Notable Albums" subtitle="Curated 24-bit and Dolby Atmos releases">
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
      {#each libraryStore.albums as album (album.id)}
        <button
          type="button"
          onclick={() => goAlbum(album.id)}
          class="group cursor-pointer rounded-[14px] p-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
        >
          <Artwork src={album.artworkUrl} alt={album.title} radius={12} shadow="lift" class="aspect-square w-full" />
          <div class="mt-2.5 truncate text-[13px] font-semibold">{album.title}</div>
          <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{album.artist} · {album.year}</div>
        </button>
      {/each}
    </div>
  </Shelf>

  <Shelf title="Moods & Genres">
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
      {#each libraryStore.genres as genre (genre.id)}
        <button
          type="button"
          onclick={() => goGenre(genre.id)}
          class="relative h-24 cursor-pointer overflow-hidden rounded-[14px] border border-white/10 p-3.5 text-left transition-transform hover:-translate-y-0.5"
          style:background="linear-gradient(135deg, {genre.color}, #121212)"
        >
          <div class="text-[14px] font-bold text-white">{genre.name}</div>
          <div class="mt-0.5 text-[11.5px] text-white/75">{genre.subtitle}</div>
        </button>
      {/each}
    </div>
  </Shelf>

  <Shelf title="Personal Mixes" subtitle="Generated from your recent listening">
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {#each libraryStore.tracks.slice(0, 4) as track (track.id)}
        <button
          type="button"
          onclick={() => playTrackNow(track, libraryStore.tracks)}
          class="flex cursor-pointer items-center gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-2.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
        >
          <Artwork src={track.artworkUrl} size={52} radius={10} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px] font-semibold">{track.title} Radio</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">With {track.artist}</div>
          </div>
        </button>
      {/each}
    </div>
  </Shelf>
</div>
