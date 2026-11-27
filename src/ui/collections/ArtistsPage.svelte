<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import { goArtist } from '$src/ui/navigation/waveNav';
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Artists" subtitle="{libraryStore.artists.length} followed & library artists" />
  <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
    {#each libraryStore.artists as artist (artist.id)}
      <button type="button" onclick={() => goArtist(artist.id)} class="cursor-pointer rounded-[16px] p-3 text-center hover:bg-[var(--lw-wash-hover)]">
        <Artwork src={artist.artworkUrl} alt={artist.name} circle shadow="lift" class="mx-auto aspect-square w-full max-w-[156px]" />
        <div class="mt-3 truncate text-[14px] font-semibold">{artist.name}</div>
        <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
      </button>
    {/each}
  </div>
</div>
