<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import { goAlbum } from '$src/ui/navigation/waveNav';
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Albums" subtitle="{libraryStore.albums.length} albums in your library" />
  <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
    {#each libraryStore.albums as album (album.id)}
      <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[14px] p-2 text-left hover:bg-[var(--lw-wash-hover)]">
        <Artwork src={album.artworkUrl} alt={album.title} radius={12} shadow="lift" class="aspect-square w-full" />
        <div class="mt-2.5 truncate text-[13px] font-semibold">{album.title}</div>
        <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{album.artist} · {album.year}</div>
      </button>
    {/each}
  </div>
</div>
