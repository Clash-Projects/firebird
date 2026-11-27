<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';
  import LibraryFolders from './LibraryFolders.svelte';

  interface Props {
    tab: 'songs' | 'albums' | 'artists' | 'folders';
  }

  let { tab }: Props = $props();
</script>

{#if tab === 'songs'}
  <DesktopTable tracks={libraryStore.tracks} />
{:else if tab === 'albums'}
  <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
    {#each libraryStore.albums as album (album.id)}
      <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[14px] p-2 text-left hover:bg-[var(--lw-wash-hover)]">
        <Artwork src={album.artworkUrl} radius={12} class="aspect-square w-full" />
        <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
        <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{album.artist}</div>
      </button>
    {/each}
  </div>
{:else if tab === 'artists'}
  <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
    {#each libraryStore.artists as artist (artist.id)}
      <button type="button" onclick={() => goArtist(artist.id)} class="cursor-pointer rounded-[14px] p-2 text-center hover:bg-[var(--lw-wash-hover)]">
        <Artwork src={artist.artworkUrl} circle class="mx-auto aspect-square w-full max-w-[140px]" />
        <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
        <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
      </button>
    {/each}
  </div>
{:else}
  <LibraryFolders />
{/if}
