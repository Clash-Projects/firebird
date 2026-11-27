<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import States from '$src/ui/components/States.svelte';
  import DownloadsPage from '$src/ui/collections/DownloadsPage.svelte';
  import { goAlbum, goArtist, goPlaylist } from '$src/ui/navigation/waveNav';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
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
  <div class="px-6 pt-6 pb-1 md:px-8">
    <h1 class="text-[22px] font-bold tracking-[-0.4px] leading-tight text-[var(--lw-text-primary)]">Library</h1>
    <div class="mt-3 flex flex-wrap items-center border-b border-[var(--lw-divider)]">
      {#each TABS as item (item.id)}
        {@const active = tab === item.id}
        <button
          type="button"
          onclick={() => (tab = item.id)}
          aria-pressed={active}
          class="mr-5 cursor-pointer border-b-2 px-0.5 py-2 text-[13.5px] transition-colors duration-[110ms] {active
            ? 'border-[var(--lw-accent)] font-semibold text-[var(--lw-text-primary)]'
            : 'border-transparent font-normal text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
        >
          {item.label}
        </button>
      {/each}
    </div>
  </div>

  <div use:waveSmoothScroll class="lw-scroll flex-1 px-6 py-4 pb-10 md:px-8">
    {#if tab === 'songs'}
      {#if libraryStore.tracks.length}
        <DesktopTable tracks={libraryStore.tracks} />
      {:else}
        <States icon="music" title="No songs yet" description="Find music online or add a folder from your computer.">
          {#snippet action()}
            <div class="flex flex-wrap items-center justify-center gap-2">
              <button type="button" onclick={() => router.navigate('/search')} class="cursor-pointer rounded-[9px] bg-[var(--lw-accent)] px-3 py-1.5 font-semibold text-[var(--lw-on-accent)]">Find music</button>
              <button type="button" onclick={() => (tab = 'folders')} class="cursor-pointer rounded-[9px] border border-[var(--lw-outline)] px-3 py-1.5 font-semibold hover:bg-[var(--lw-wash-hover)]">Add a folder</button>
            </div>
          {/snippet}
        </States>
      {/if}
    {:else if tab === 'albums'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each libraryStore.albums as album (album.id)}
          <button type="button" onclick={() => goAlbum(album.id)} class="group cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
            <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" alt={album.title} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
          </button>
        {/each}
      </div>
    {:else if tab === 'artists'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each libraryStore.artists as artist (artist.id)}
          <button type="button" onclick={() => goArtist(artist.id)} class="group cursor-pointer rounded-[12px] p-2 text-center transition-colors hover:bg-[var(--lw-wash-hover)]">
            <Artwork src={artist.artworkUrl} label={artist.name} kind="artist" alt={artist.name} circle shadow="lift" class="mx-auto aspect-square w-full max-w-[140px] transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2.5 truncate text-[13px] font-semibold">{artist.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{artist.monthlyListeners}</div>
          </button>
        {/each}
      </div>
    {:else if tab === 'playlists'}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each localPlaylists as pl (pl.id)}
          <button type="button" onclick={() => goPlaylist(pl.id, pl.source)} class="group cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
            <Artwork src={pl.artworkUrl} label={pl.name} kind="album" alt={pl.name} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
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
