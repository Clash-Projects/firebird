<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import DownloadsPage from '$src/ui/collections/DownloadsPage.svelte';
  import { goAlbum, goArtist, goPlaylist } from '$src/ui/navigation/waveNav';
  import LibraryFolders from './LibraryFolders.svelte';

  type LibraryTab = 'songs' | 'albums' | 'artists' | 'playlists' | 'downloads' | 'folders';

  const TABS: ReadonlyArray<{ id: LibraryTab; label: string }> = [
    { id: 'songs', label: 'Songs' },
    { id: 'albums', label: 'Albums' },
    { id: 'artists', label: 'Artists' },
    { id: 'playlists', label: 'Playlists' },
    { id: 'downloads', label: 'Downloads' },
    { id: 'folders', label: 'Folders' },
  ];

  let tab = $state<LibraryTab>('songs');
  let localPlaylists = $derived(libraryStore.playlists.filter((p) => p.source === 'local'));
</script>

<div class="flex h-full flex-col">
  <div class="px-6 pt-5 pb-1">
    <h1 class="text-[22px] font-bold tracking-[-0.4px] leading-[1.15]">Library</h1>
    <div class="mt-2.5 flex items-center border-b border-[var(--lw-divider)]">
      {#each TABS as item (item.id)}
        {@const active = tab === item.id}
        <button
          type="button"
          onclick={() => (tab = item.id)}
          class="mr-5 cursor-pointer border-b-2 px-0.5 py-2 text-[13.5px] transition-colors duration-[110ms] {active
            ? 'border-[var(--lw-accent)] font-semibold text-[var(--lw-text-primary)]'
            : 'border-transparent font-normal text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
        >
          {item.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="lw-scroll flex-1 px-6 py-4 pb-10">
    {#if tab === 'songs'}
      <DesktopTable tracks={libraryStore.tracks} />
    {:else if tab === 'albums'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each libraryStore.albums as album (album.id)}
          <button type="button" onclick={() => goAlbum(album.id)} class="group cursor-pointer rounded-[14px] p-1.5 text-left transition-colors hover:bg-white/[0.04]">
            <Artwork src={album.artworkUrl} alt={album.title} radius={12} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
          </button>
        {/each}
      </div>
    {:else if tab === 'artists'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each libraryStore.artists as artist (artist.id)}
          <button type="button" onclick={() => goArtist(artist.id)} class="group cursor-pointer rounded-[14px] p-2 text-center transition-colors hover:bg-white/[0.04]">
            <Artwork src={artist.artworkUrl} alt={artist.name} circle shadow="lift" class="mx-auto aspect-square w-full max-w-[140px] transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2.5 truncate text-[13px] font-semibold">{artist.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{artist.monthlyListeners}</div>
          </button>
        {/each}
      </div>
    {:else if tab === 'playlists'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each localPlaylists as pl (pl.id)}
          <button type="button" onclick={() => goPlaylist(pl.id, pl.source)} class="group cursor-pointer rounded-[14px] p-1.5 text-left transition-colors hover:bg-white/[0.04]">
            <Artwork src={pl.artworkUrl} alt={pl.name} radius={12} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{pl.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{pl.trackIds.length} tracks</div>
          </button>
        {/each}
      </div>
    {:else if tab === 'downloads'}
      <DownloadsPage />
    {:else}
      <div class="max-w-[720px]">
        <LibraryFolders />
      </div>
    {/if}
  </div>
</div>
