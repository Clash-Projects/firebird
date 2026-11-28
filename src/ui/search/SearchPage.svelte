<script lang="ts">
  import { untrack } from 'svelte';
  import { isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { exactArtist, fetchSearchResults, openSearchResult, rememberSearchResults, searchMatches, searchResultDetails, type SearchResults } from './searchResults';

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
  let remoteSearch = $state<{ query: string; results: SearchResults } | null>(null);

  let q = $derived(query.trim());
  let matches = $derived(searchMatches(q, remoteSearch?.query === q ? remoteSearch.results : undefined));
  let tracks = $derived(matches.flatMap((match) => match.kind === 'track' ? [match.item] : []));
  let albums = $derived(matches.flatMap((match) => match.kind === 'album' ? [match.item] : []));
  let artists = $derived(matches.flatMap((match) => match.kind === 'artist' ? [match.item] : []));
  let sectionOrder = $derived(Array.from(new Set(matches.map((match) => match.kind))));

  $effect(() => {
    const trimmed = q;
    searching = Boolean(trimmed && isTauriRuntime());
    if (!trimmed || !isTauriRuntime()) return;
    let current = true;
    const timer = setTimeout(async () => {
      const results = await fetchSearchResults(trimmed, { tracks: 25, albums: 12, artists: 12 });
      if (!current) return;
      rememberSearchResults(results);
      remoteSearch = { query: trimmed, results };
      searching = false;
    }, 280);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  });

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    if (filter === 'all' || filter === 'artists') {
      const artist = exactArtist(matches);
      if (artist) openSearchResult({ kind: 'artist', item: artist }, q);
    }
  }
</script>

<div class="mx-auto w-full max-w-[1440px] space-y-6 px-6 py-6 pb-12 md:px-8">
  <h1 class="text-[22px] font-bold tracking-[-0.4px] leading-tight text-[var(--lw-text-primary)]">Search</h1>
  <form onsubmit={submitSearch} class="flex flex-wrap items-center gap-3">
    <div class="w-full max-w-[460px]">
      <Input type="search" bind:value={query} ariaLabel="Search songs, albums, and artists" placeholder="Search songs, albums, artists…" autofocus />
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
        <ul aria-label="Recently searched" class="recent-search-grid">
          {#each libraryStore.searchHistory as entry (entry.key)}
            {@const detail = entry.kind === 'query' ? { label: entry.query, sub: 'Search', art: '' } : searchResultDetails(entry)}
            <li class="recent-search-row">
              <button
                type="button"
                class="recent-search-open"
                title={detail.label}
                onclick={() => entry.kind === 'query' ? (query = entry.query) : openSearchResult(entry)}
              >
                {#if entry.kind === 'query'}
                  <span class="recent-search-query-icon"><WaveIcon name="history" size={17} /></span>
                {:else}
                  <Artwork src={detail.art} label={detail.label} kind={entry.kind} size={36} radius={6} circle={entry.kind === 'artist'} upgrade={false} />
                {/if}
                <span class="min-w-0">
                  <span class="block truncate text-[13px] font-semibold text-[var(--lw-text-primary)]">{detail.label}</span>
                  <span class="block truncate text-[11.5px] text-[var(--lw-text-secondary)]">{detail.sub}</span>
                </span>
              </button>
              <button
                type="button"
                aria-label={`Remove ${detail.label} from recent searches`}
                onclick={() => libraryStore.removeSearchHistory(entry)}
                class="recent-search-remove"
              >
                <WaveIcon name="close" size={12} />
              </button>
            </li>
          {/each}
        </ul>
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
    {#each sectionOrder as kind (kind)}
      {#if kind === 'track' && (filter === 'all' || filter === 'tracks')}
        <Shelf title="Songs">
          <DesktopTable {tracks} showQuality={false} onPlay={(track, list) => openSearchResult({ kind: 'track', item: track }, q, list)} />
        </Shelf>
      {:else if kind === 'album' && (filter === 'all' || filter === 'albums')}
        <Shelf title="Albums">
          <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
            {#each albums as album (album.id)}
              <button type="button" onclick={() => openSearchResult({ kind: 'album', item: album }, q)} class="cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
                <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" radius={10} class="aspect-square w-full" />
                <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
              </button>
            {/each}
          </div>
        </Shelf>
      {:else if kind === 'artist' && (filter === 'all' || filter === 'artists')}
        <Shelf title="Artists">
          <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
            {#each artists as artist (artist.id)}
              <button type="button" onclick={() => openSearchResult({ kind: 'artist', item: artist }, q)} class="cursor-pointer rounded-[12px] p-2 text-center transition-colors hover:bg-[var(--lw-wash-hover)]">
                <Artwork src={artist.artworkUrl} label={artist.name} kind="artist" circle class="mx-auto aspect-square w-full max-w-[140px]" />
                <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
              </button>
            {/each}
          </div>
        </Shelf>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .recent-search-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 220px), 1fr));
    gap: 8px;
  }
  .recent-search-row {
    display: flex;
    min-width: 0;
    align-items: center;
    padding-inline-end: 5px;
    border: 1px solid var(--lw-outline);
    border-radius: 9px;
    background: var(--lw-surface);
  }
  .recent-search-row:hover, .recent-search-row:focus-within { background: var(--lw-surface-raised); }
  .recent-search-open {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 9px;
    padding: 6px 7px;
    border-radius: 8px;
    cursor: pointer;
    text-align: start;
  }
  .recent-search-query-icon {
    display: grid;
    width: 36px;
    height: 36px;
    flex: none;
    place-items: center;
    color: var(--lw-text-secondary);
  }
  .recent-search-remove {
    display: grid;
    width: 28px;
    height: 28px;
    flex: none;
    place-items: center;
    border-radius: 5px;
    color: var(--lw-text-secondary);
    cursor: pointer;
  }
  .recent-search-remove:hover { background: var(--lw-wash-hover); color: var(--lw-text-primary); }

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
