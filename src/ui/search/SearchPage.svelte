<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  const BROWSE_CATEGORIES = [
    { name: 'Hip-Hop/Rap', tint: '#C45C1A', query: 'hip hop' },
    { name: 'Pop', tint: '#C45A88', query: 'pop' },
    { name: 'R&B', tint: '#7A4FC4', query: 'r&b' },
    { name: 'Electronic', tint: '#1A8A8A', query: 'electronic' },
    { name: 'Rock', tint: '#B03A4A', query: 'rock' },
    { name: 'Jazz', tint: '#2E6B8A', query: 'jazz' },
    { name: 'Indie', tint: '#3D8A62', query: 'indie' },
    { name: 'K-Pop', tint: '#C44A7A', query: 'k-pop' },
    { name: 'Latin', tint: '#C46A2A', query: 'latin' },
    { name: 'Classical', tint: '#5A6A7A', query: 'classical' },
    { name: 'Country', tint: '#8A6A2E', query: 'country' },
    { name: 'Afrobeats', tint: '#C48A1A', query: 'afrobeats' },
    { name: 'Charts', tint: '#4A6AC4', query: 'top' },
    { name: 'Chill', tint: '#3A7A8A', query: 'chill' },
    { name: 'Workout', tint: '#C43A3A', query: 'workout' },
  ];

  let query = $state('');
  let filter = $state<'all' | 'tracks' | 'albums' | 'artists'>('all');

  let q = $derived(query.trim().toLowerCase());
  let tracks = $derived(
    q
      ? libraryStore.tracks.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            t.artist.toLowerCase().includes(q) ||
            t.album.toLowerCase().includes(q)
        )
      : []
  );
  let albums = $derived(
    q
      ? libraryStore.albums.filter(
          (a) => a.title.toLowerCase().includes(q) || a.artist.toLowerCase().includes(q)
        )
      : []
  );
  let artists = $derived(
    q ? libraryStore.artists.filter((a) => a.name.toLowerCase().includes(q)) : []
  );

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    libraryStore.pushSearchHistory(query);
  }
</script>

<div class="w-full space-y-7 px-6 py-5 pb-12">
  <form onsubmit={submitSearch} class="flex flex-wrap items-center gap-3">
    <div class="w-full max-w-[460px]">
      <Input bind:value={query} placeholder="Search songs, albums, artists..." autofocus />
    </div>
    {#if q}
      <div class="flex items-center gap-1.5">
        {#each (['all', 'tracks', 'albums', 'artists'] as const) as tab (tab)}
          <Buttons variant="chip" size="sm" active={filter === tab} onclick={() => (filter = tab)}>
            {tab[0].toUpperCase() + tab.slice(1)}
          </Buttons>
        {/each}
      </div>
    {/if}
  </form>

  {#if !q}
    {#if libraryStore.searchHistory.length > 0}
      <Shelf title="Recently Searched" actionLabel="Clear" onAction={() => libraryStore.clearSearchHistory()}>
        <div class="flex flex-wrap gap-2">
          {#each libraryStore.searchHistory as item (item)}
            <div class="inline-flex items-center gap-1.5 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-3 py-1 text-[12.5px]">
              <button type="button" onclick={() => (query = item)} class="cursor-pointer hover:text-[var(--lw-accent)]">
                {item}
              </button>
              <button
                type="button"
                onclick={() => libraryStore.removeSearchHistory(item)}
                class="cursor-pointer text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]"
              >
                <WaveIcon name="close" size={12} />
              </button>
            </div>
          {/each}
        </div>
      </Shelf>
    {/if}

    <Shelf title="Browse Categories">
      <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
        {#each BROWSE_CATEGORIES as cat (cat.name)}
          <button
            type="button"
            onclick={() => (query = cat.query)}
            class="h-[78px] cursor-pointer rounded-[14px] border border-white/10 px-4 py-3 text-left transition-transform duration-[110ms] hover:-translate-y-0.5"
            style:background="linear-gradient(135deg, {cat.tint}, color-mix(in srgb, {cat.tint} 32%, #0e0e0e))"
          >
            <div class="text-[14px] font-bold text-white">{cat.name}</div>
          </button>
        {/each}
      </div>
    </Shelf>
  {:else if tracks.length === 0 && albums.length === 0 && artists.length === 0}
    <States icon="search" title="No matches found" description={`No results for "${query}". Try another track or artist name.`} />
  {:else}
    {#if (filter === 'all' || filter === 'tracks') && tracks.length > 0}
      <Shelf title="Songs">
        <DesktopTable {tracks} />
      </Shelf>
    {/if}

    {#if (filter === 'all' || filter === 'albums') && albums.length > 0}
      <Shelf title="Albums">
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {#each albums as album (album.id)}
            <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[14px] p-1.5 text-left hover:bg-white/[0.04]">
              <Artwork src={album.artworkUrl} radius={12} class="aspect-square w-full" />
              <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
            </button>
          {/each}
        </div>
      </Shelf>
    {/if}

    {#if (filter === 'all' || filter === 'artists') && artists.length > 0}
      <Shelf title="Artists">
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {#each artists as artist (artist.id)}
            <button type="button" onclick={() => goArtist(artist.id)} class="cursor-pointer rounded-[14px] p-2 text-center hover:bg-white/[0.04]">
              <Artwork src={artist.artworkUrl} circle class="mx-auto aspect-square w-full max-w-[140px]" />
              <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
            </button>
          {/each}
        </div>
      </Shelf>
    {/if}
  {/if}
</div>
