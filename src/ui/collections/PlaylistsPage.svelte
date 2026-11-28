<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import type { Playlist } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { goPlaylist } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import PlaylistDialogs from './PlaylistDialogs.svelte';

  const sources = [
    { id: 'local', filterLabel: 'Local', title: 'Your playlists' },
    { id: 'youtube', filterLabel: 'YouTube Music', title: 'YouTube Music' },
    { id: 'spotify', filterLabel: 'Spotify', title: 'Spotify' },
  ] as const;

  type Source = (typeof sources)[number]['id'];
  type Filter = Source | 'all';

  let createOpen = $state(false);
  let filter = $state('');
  let sourceFilter = $state<Filter>('all');

  function playlistSource(playlist: Playlist) {
    return playlist.id === 'pl-liked' ? 'youtube' : playlist.source;
  }

  let playlists = $derived(libraryStore.playlists.filter((playlist) => playlistSource(playlist) !== 'generated'));
  let availableSources = $derived(sources.filter((source) => playlists.some((playlist) => playlistSource(playlist) === source.id)));
  let selectedSource = $derived(availableSources.some((source) => source.id === sourceFilter) ? sourceFilter : 'all');
  let query = $derived(filter.trim().toLowerCase());
  let visible = $derived(playlists.filter((playlist) =>
    (selectedSource === 'all' || playlistSource(playlist) === selectedSource)
      && (!query || playlist.name.toLowerCase().includes(query)),
  ));
  let sections = $derived([
    { id: 'pinned', title: 'Pinned', playlists: visible.filter((playlist) => playlist.pinned) },
    ...availableSources.map((source) => ({
      id: source.id,
      title: source.title,
      playlists: visible.filter((playlist) => !playlist.pinned && playlistSource(playlist) === source.id),
    })),
  ].filter((section) => section.playlists.length));

  function openPlaylist(playlist: Playlist) {
    if (playlist.id === 'pl-liked') router.navigate('/liked');
    else goPlaylist(playlist.id, playlist.source);
  }

  function clearFilters() {
    filter = '';
    sourceFilter = 'all';
  }
</script>

<div class="playlists-page">
  <header class="playlists-heading">
    <div>
      <h1>Playlists</h1>
      {#if playlists.length}
        <p>{playlists.length} {playlists.length === 1 ? 'playlist' : 'playlists'}</p>
      {/if}
    </div>
    {#if playlists.length}
      <Buttons variant="primary" onclick={() => (createOpen = true)}>
        <WaveIcon name="plus" size={16} /> New playlist
      </Buttons>
    {/if}
  </header>

  {#if playlists.length === 0}
    <section class="playlist-starter" aria-labelledby="playlist-starter-title">
      <div class="starter-copy">
        <h2 id="playlist-starter-title">Make your first playlist</h2>
        <p>Right-click a song in search or your library to add it to a playlist.</p>
        <Buttons variant="primary" onclick={() => (createOpen = true)}>
          <WaveIcon name="plus" size={16} /> New playlist
        </Buttons>
      </div>
      <div class="starter-art" aria-hidden="true">
        <div class="record"></div>
        <div class="record-sleeve">
          <WaveIcon name="playlists" size={48} />
          <span></span>
        </div>
      </div>
    </section>
  {:else}
    <div class="playlist-toolbar">
      <div class="playlist-search">
        <Input type="search" bind:value={filter} prefixIcon="search" ariaLabel="Search playlists" placeholder="Search playlists" />
      </div>
      {#if availableSources.length > 1}
        <div class="playlist-filters" role="group" aria-label="Filter playlists by source">
          <Buttons variant="toggle" size="sm" active={selectedSource === 'all'} onclick={() => (sourceFilter = 'all')}>All</Buttons>
          {#each availableSources as source (source.id)}
            <Buttons variant="toggle" size="sm" active={selectedSource === source.id} onclick={() => (sourceFilter = source.id)}>
              {source.filterLabel}
            </Buttons>
          {/each}
        </div>
      {/if}
    </div>

    {#if sections.length === 0}
      <section class="playlist-no-results" aria-labelledby="playlist-no-results-title">
        <h2 id="playlist-no-results-title">No matching playlists</h2>
        <p>Try a different name or clear your filters.</p>
        <Buttons variant="secondary" size="sm" onclick={clearFilters}>Clear filters</Buttons>
      </section>
    {:else}
      <div class="playlist-sections">
        {#each sections as section (section.id)}
          <section aria-labelledby={"playlists-" + section.id}>
            <h2 class="playlist-section-title" id={"playlists-" + section.id}>{section.title}</h2>
            <ul class="playlist-grid" class:pinned-grid={section.id === 'pinned'}>
              {#each section.playlists as playlist (playlist.id)}
                {@const cover = playlist.artworkUrl || libraryStore.trackById(playlist.trackIds[0] ?? '')?.artworkUrl}
                {@const source = playlistSource(playlist)}
                <li>
                  <button
                    class="playlist-card"
                    class:pinned-card={section.id === 'pinned'}
                    type="button"
                    onclick={() => openPlaylist(playlist)}
                  >
                    <div class="playlist-cover" aria-hidden="true">
                      {#if cover}
                        <Artwork src={cover} kind="album" upgrade={false} radius={0} class="h-full w-full" />
                      {:else}
                        <WaveIcon name={playlist.id === 'pl-liked' ? 'heart' : 'playlists'} size={section.id === 'pinned' ? 23 : 36} />
                      {/if}
                    </div>
                    <span class="playlist-label">
                      <span class="playlist-name" title={playlist.name}>{playlist.name}</span>
                      {#if source === 'local' || playlist.trackIds.length}
                        <span class="playlist-meta">{playlist.trackIds.length} {playlist.trackIds.length === 1 ? 'track' : 'tracks'}</span>
                      {:else if section.id === 'pinned'}
                        <span class="playlist-meta">{sources.find((entry) => entry.id === source)?.title}</span>
                      {/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<PlaylistDialogs
  open={createOpen}
  onClose={() => (createOpen = false)}
  onCreated={(id) => goPlaylist(id)}
/>

<style>
  .playlists-page {
    box-sizing: border-box;
    width: 100%;
    max-width: 1500px;
    margin-inline: auto;
    padding: clamp(24px, 3.5vw, 54px) clamp(20px, 3.6vw, 56px) 56px;
    container: playlists / inline-size;
  }

  .playlists-heading {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px 20px;
    margin-bottom: 24px;
  }

  .playlists-heading h1 {
    font-size: clamp(34px, 5cqw, 62px);
    font-weight: 750;
    letter-spacing: -0.055em;
    line-height: 1.03;
  }

  .playlists-heading p {
    margin-top: 8px;
    color: var(--lw-text-secondary);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .playlist-starter {
    --sleeve-tint: #abc17e;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(220px, 0.8fr);
    align-items: center;
    gap: 22px 32px;
    padding: clamp(26px, 4.4cqw, 62px);
    border: 1px solid var(--lw-outline-soft);
    border-radius: 18px;
    background: radial-gradient(ellipse at 82% 58%, color-mix(in srgb, var(--sleeve-tint) 13%, transparent), transparent 53%),
      color-mix(in srgb, var(--lw-surface-raised) 84%, var(--lw-bg));
  }

  .starter-copy h2 {
    font-size: clamp(24px, 3.2cqw, 40px);
    font-weight: 700;
    line-height: 1.12;
    letter-spacing: -0.045em;
    text-wrap: balance;
  }

  .starter-copy p,
  .playlist-no-results p {
    margin: 8px 0 20px;
    max-width: 34ch;
    color: var(--lw-text-secondary);
    font-size: 14px;
    line-height: 1.55;
  }

  .starter-art {
    position: relative;
    justify-self: center;
    width: min(100%, 330px);
    aspect-ratio: 330 / 225;
  }

  .record,
  .record-sleeve {
    position: absolute;
    top: 5%;
    height: 90%;
    aspect-ratio: 1;
  }

  .record {
    right: 3%;
    border-radius: 50%;
    border: 1px solid #575c50;
    background: radial-gradient(circle, #20251b 2.5%, #a7ba83 3% 18%, transparent 18.5%),
      repeating-radial-gradient(circle, transparent 0 2px, rgb(228 239 205 / 9%) 3px 4px),
      conic-gradient(from 14deg, #22261f, #444b3d, #191d17, #3e4538, #22261f);
    box-shadow: 0 12px 32px rgb(12 16 9 / 35%);
  }

  .record-sleeve {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    left: 6%;
    border: 1px solid rgb(226 238 193 / 20%);
    border-radius: 9px;
    background: radial-gradient(circle at 10% 8%, #596845, transparent 64%), #303929;
    color: #e0edc9;
    transform: rotate(-8deg);
    box-shadow: 0 15px 34px rgb(12 16 9 / 45%);
  }

  .record-sleeve span {
    width: 27px;
    height: 1px;
    background: rgb(224 237 201 / 48%);
  }

  .playlist-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 18px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--lw-divider);
  }

  .playlist-search {
    flex: 1 1 230px;
    max-width: 320px;
  }

  .playlist-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .playlist-sections {
    display: grid;
    gap: 28px;
    padding-top: 23px;
  }

  .playlist-section-title,
  .playlist-no-results h2 {
    font-size: 21px;
    font-weight: 700;
    letter-spacing: -0.035em;
  }

  .playlist-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 150px), 1fr));
    gap: 16px;
    margin-top: 12px;
  }

  .playlist-grid li {
    min-width: 0;
  }

  .pinned-grid {
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 226px), 1fr));
    gap: 10px;
  }

  .playlist-card {
    display: block;
    width: 100%;
    padding: 6px 6px 9px;
    border-radius: 12px;
    text-align: start;
    cursor: pointer;
    transition: background-color var(--lw-motion-fast) var(--ease-wave);
  }

  .playlist-card:is(:hover, :focus-visible) {
    background: var(--lw-wash-hover);
  }

  .pinned-card {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    min-height: 64px;
    padding: 6px 10px 6px 6px;
    background: var(--lw-surface);
  }

  .playlist-cover {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    overflow: hidden;
    border-radius: 8px;
    background: radial-gradient(at left top, color-mix(in srgb, var(--lw-text-primary) 10%, transparent), transparent 80%),
      var(--lw-surface-raised);
    color: var(--lw-text-secondary);
  }

  .playlist-cover :global([data-lw="artwork"]) {
    grid-area: 1 / 1;
    min-width: 0;
    min-height: 0;
  }

  .playlist-label {
    display: block;
    min-width: 0;
  }

  .playlist-name,
  .playlist-meta {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pinned-card .playlist-name {
    margin-top: 0;
  }

  .playlist-name {
    margin-top: 8px;
    color: var(--lw-text-primary);
    font-size: 14px;
    font-weight: 650;
  }

  .playlist-meta {
    margin-top: 2px;
    color: var(--lw-text-secondary);
    font-size: 12px;
  }

  .playlist-no-results {
    padding-top: 32px;
  }

  @container playlists (width < 590px) {
    .playlist-starter {
      grid-template-columns: minmax(0, 1fr);
    }

    .starter-art {
      justify-self: end;
      width: min(100%, 224px);
    }
  }

  @container playlists (width < 380px) {
    .playlist-grid:not(.pinned-grid) {
      grid-template-columns: repeat(auto-fill, minmax(min(100%, 122px), 1fr));
      gap: 10px;
    }
  }
</style>
