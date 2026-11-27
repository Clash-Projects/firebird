<script lang="ts">
  import { untrack } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtAlbum, mapYtArtist, mapYtTrack } from '$src/state/library.svelte';
  import type { Album, Artist, Track } from '$src/state/types';
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

  let { initialQuery = '' }: { initialQuery?: string } = $props();
  let query = $state(untrack(() => initialQuery));
  let filter = $state<'all' | 'tracks' | 'albums' | 'artists'>('all');
  let searching = $state(false);
  let remoteTracks = $state<Track[]>([]);
  let remoteAlbums = $state<Album[]>([]);
  let remoteArtists = $state<Artist[]>([]);
  let searchSeq = 0;

  let q = $derived(query.trim().toLowerCase());

  $effect(() => {
    const trimmed = query.trim();
    const seq = ++searchSeq;
    remoteTracks = [];
    remoteAlbums = [];
    remoteArtists = [];
    if (!trimmed) {
      searching = false;
      return;
    }
    if (!isTauriRuntime()) return;

    searching = true;
    const timer = setTimeout(async () => {
      try {
        const [songsRes, albumsRes, artistsRes] = await Promise.all([
          ipcInvoke<any[]>('yt_search_songs', { query: trimmed, limit: 25 }),
          ipcInvoke<any[]>('yt_search_albums', { query: trimmed, limit: 12 }),
          ipcInvoke<any[]>('yt_search_artists', { query: trimmed, limit: 12 }),
        ]);
        if (seq !== searchSeq) return;
        const mappedSongs = Array.isArray(songsRes) ? songsRes.map(mapYtTrack) : [];
        const mappedAlbums = Array.isArray(albumsRes) ? albumsRes.map(mapYtAlbum) : [];
        const mappedArtists = Array.isArray(artistsRes) ? artistsRes.map(mapYtArtist) : [];

        remoteTracks = mappedSongs;
        remoteAlbums = mappedAlbums;
        remoteArtists = mappedArtists;
        libraryStore.upsertTracks(mappedSongs);
        libraryStore.upsertAlbums(mappedAlbums);
        libraryStore.upsertArtists(mappedArtists);
      } finally {
        if (seq === searchSeq) searching = false;
      }
    }, 280);

    return () => clearTimeout(timer);
  });

  let tracks = $derived(
    remoteTracks.length > 0
      ? remoteTracks
      : q
        ? libraryStore.tracks.filter(
            (t) =>
              t.title.toLowerCase().includes(q) ||
              t.artist.toLowerCase().includes(q) ||
              t.album.toLowerCase().includes(q)
          )
        : []
  );
  let albums = $derived(
    remoteAlbums.length > 0
      ? remoteAlbums
      : q
        ? libraryStore.albums.filter(
            (a) => a.title.toLowerCase().includes(q) || a.artist.toLowerCase().includes(q)
          )
        : []
  );
  let artists = $derived(
    remoteArtists.length > 0
      ? remoteArtists
      : q
        ? libraryStore.artists.filter((a) => a.name.toLowerCase().includes(q))
        : []
  );

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    libraryStore.pushSearchHistory(query);
  }
</script>

<div class="mx-auto w-full max-w-[1440px] space-y-6 px-6 py-6 pb-12 md:px-8">
  <h1 class="text-[22px] font-bold tracking-[-0.4px] leading-tight text-[var(--lw-text-primary)]">Search</h1>
  <form onsubmit={submitSearch} class="flex flex-wrap items-center gap-3">
    <div class="w-full max-w-[460px]">
      <Input bind:value={query} ariaLabel="Search songs, albums, and artists" placeholder="Search songs, albums, artists…" autofocus />
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
                aria-label={`Remove ${item} from recent searches`}
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

    <Shelf title="Browse genres & moods">
      <div class="genre-grid">
        {#each BROWSE_CATEGORIES as cat (cat.name)}
          <button
            type="button"
            onclick={() => (query = cat.query)}
            class="genre-tile"
            style:--genre-tint={cat.tint}
          >
            <span class="relative z-10">{cat.name}</span>
            <span class="genre-cue"><WaveIcon name="arrowRight" size={15} /></span>
          </button>
        {/each}
      </div>
    </Shelf>
  {:else if searching && tracks.length === 0 && albums.length === 0 && artists.length === 0}
    <div class="space-y-2 text-[13px] text-[var(--lw-text-secondary)]" role="status">
      <p>Searching YouTube Music for “{query}”…</p>
      {#each Array(3) as _}
        <div class="lw-skeleton h-10 rounded-[9px]" aria-hidden="true"></div>
      {/each}
    </div>
  {:else if tracks.length === 0 && albums.length === 0 && artists.length === 0}
    <States icon="search" title="No matches found" description={`No results for "${query}". Try another track or artist name.`} />
  {:else}
    {#if (filter === 'all' || filter === 'tracks') && tracks.length > 0}
      <Shelf title="Songs">
        <DesktopTable {tracks} showQuality={false} />
      </Shelf>
    {/if}

    {#if (filter === 'all' || filter === 'albums') && albums.length > 0}
      <Shelf title="Albums">
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {#each albums as album (album.id)}
            <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
              <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" radius={10} class="aspect-square w-full" />
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
            <button type="button" onclick={() => goArtist(artist.id)} class="cursor-pointer rounded-[12px] p-2 text-center transition-colors hover:bg-[var(--lw-wash-hover)]">
              <Artwork src={artist.artworkUrl} label={artist.name} kind="artist" circle class="mx-auto aspect-square w-full max-w-[140px]" />
              <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
            </button>
          {/each}
        </div>
      </Shelf>
    {/if}
  {/if}
</div>

<style>
  .genre-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(141px, 1fr)); gap: 10px; }
  .genre-tile {
    position: relative;
    display: flex;
    min-height: 81px;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    overflow: hidden;
    padding: 13px;
    border: 1px solid color-mix(in srgb, var(--genre-tint) 30%, var(--lw-outline));
    border-radius: 12px;
    background: color-mix(in srgb, var(--genre-tint) 13%, var(--lw-surface));
    cursor: pointer;
    font-size: 14px;
    font-weight: 700;
    text-align: left;
    transition: border-color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }
  .genre-tile::after {
    position: absolute;
    right: -13px;
    bottom: -30px;
    width: 98px;
    aspect-ratio: 1;
    border: 1px solid color-mix(in srgb, var(--genre-tint) 49%, transparent);
    border-radius: 50%;
    background: repeating-radial-gradient(circle, transparent 0 6px, color-mix(in srgb, var(--genre-tint) 32%, transparent) 7px 8px);
    content: '';
    transition: transform var(--lw-motion-normal) var(--ease-wave);
  }
  .genre-tile:active { transform: scale(calc(1 - 0.02 * var(--lw-motion-factor))); }
  .genre-cue { display: flex; flex: none; color: var(--lw-text-secondary); }
  @media (hover: hover) and (pointer: fine) {
    .genre-tile:hover { border-color: color-mix(in srgb, var(--genre-tint) 65%, var(--lw-outline)); }
    .genre-tile:hover::after { transform: translate(calc(-4px * var(--lw-motion-factor)), calc(-3px * var(--lw-motion-factor))); }
  }
</style>
