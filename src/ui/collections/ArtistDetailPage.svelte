<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtArtist, mapYtTrack } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { Album, Track } from '$src/state/types';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import ShelfRail from '$src/ui/components/ShelfRail.svelte';
  import Skeletons from '$src/ui/components/Skeletons.svelte';
  import States from '$src/ui/components/States.svelte';
  import TrackRow from '$src/ui/components/TrackRow.svelte';
  import { goAlbum, goArtistPopularTracks } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import ArtistPopularTracksPage from './ArtistPopularTracksPage.svelte';
  import { loadArtistBannerMotion, subscribeToArtistPage, type ArtistPageResponse } from './artistPageLoader';
  import { readArtistReleases, selectEssentialAlbums, selectLatestAlbum } from './artistReleases';

  const TRACKS_PER_COLUMN = 3;
  const TRACK_PREVIEW_COUNT = 18;

  let { artistId, showPopularTracks = false }: { artistId: string; showPopularTracks?: boolean } = $props();
  const id = $props.id();
  let loading = $state(true);
  let remoteSongs = $state<Track[]>([]);
  let remoteCatalog = $state<ReturnType<typeof readArtistReleases> | null>(null);
  let bannerUrl = $state('');
  let stillArtwork = $state<{ artistId: string; url: string } | null>(null);
  let systemReduceMotion = $state(true);
  let artistMotion = $state<{ artistId: string; url: string | null } | null>(null);

  let artist = $derived(libraryStore.artists.find((entry) => entry.id === artistId));
  let profileArtwork = $derived(artist?.bannerUrl || artist?.artworkUrl);
  let stillUrl = $derived(stillArtwork?.artistId === artistId ? stillArtwork.url : '');
  let motionEnabled = $derived(
    settingsStore.canvasEnabled && !settingsStore.reduceMotion && !settingsStore.performanceMode && !systemReduceMotion,
  );
  let motionUrl = $derived(motionEnabled && artistMotion?.artistId === artistId ? artistMotion.url : null);
  let topTracks = $derived(
    remoteSongs.length ? remoteSongs : libraryStore.tracksByIds(artist?.topTrackIds ?? [])
  );
  let discography = $derived(
    remoteCatalog?.albums.length ? remoteCatalog.albums : libraryStore.albums.filter((album) => artist?.albumIds.includes(album.id))
  );
  let trackColumns = $derived.by(() => {
    const preview = topTracks.slice(0, TRACK_PREVIEW_COUNT);
    return Array.from({ length: Math.ceil(preview.length / TRACKS_PER_COLUMN) }, (_, index) => {
      const start = index * TRACKS_PER_COLUMN;
      return preview.slice(start, start + TRACKS_PER_COLUMN);
    });
  });
  let albums = $derived(discography.filter((release) => release.type === 'Album'));
  let singles = $derived(discography.filter((release) => release.type !== 'Album'));
  let essentialAlbums = $derived(selectEssentialAlbums(albums, topTracks, remoteCatalog?.essentialIds ?? []));
  let featuredTrack = $derived(remoteCatalog?.featuredTrack);
  let latestAlbum = $derived(
    discography.find((release) => release.id === remoteCatalog?.featuredAlbumId)
    ?? (featuredTrack ? undefined : selectLatestAlbum(discography))
  );
  let latestRelease = $derived(latestAlbum ?? featuredTrack);
  let releasesLoading = $derived(loading && !remoteCatalog && !latestRelease);

  onMount(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const sync = () => { systemReduceMotion = preference.matches; };
    sync();
    preference.addEventListener('change', sync);
    return () => preference.removeEventListener('change', sync);
  });

  function artistImageAtSize(src: string, size: number) {
    try {
      const url = new URL(src);
      if (url.protocol === 'https:' && /\.(?:googleusercontent|ggpht)\.com$/.test(url.hostname)) {
        url.pathname = url.pathname.replace(/=(?:s\d+|w\d+(?:-h\d+)?|h\d+)(?:-[\w-]+)?$/, `=s${size}`);
        return url.href;
      }
    } catch {
      return src;
    }
    return src;
  }

  $effect(() => {
    const browseId = artistId;
    const sources = [bannerUrl, profileArtwork].filter((src): src is string => Boolean(src));
    const urls = [...new Set([
      ...sources.map((src) => artistImageAtSize(src, 0)),
      ...sources.map((src) => artistImageAtSize(src, 1600)),
      ...sources,
    ])];
    if (!urls.length) return;

    const image = new Image();
    image.referrerPolicy = 'no-referrer';
    let index = 0;

    function loadNext() {
      const url = urls[index++];
      if (!url || untrack(() => stillUrl) === url) return;
      image.onload = () => { stillArtwork = { artistId: browseId, url }; };
      image.onerror = loadNext;
      image.src = url;
    }

    loadNext();
    return () => {
      image.onload = null;
      image.onerror = null;
    };
  });

  $effect(() => {
    const name = artist?.name;
    const browseId = artistId;
    if (!name || !browseId || !motionEnabled || showPopularTracks || !isTauriRuntime()) return;
    if (loading && !remoteSongs.length) return;

    const trackTitles = topTracks
      .filter((track) => track.artists.includes(name) || track.artist === name)
      .slice(0, 4)
      .map((track) => track.title);

    let current = true;
    void loadArtistBannerMotion(browseId, name, trackTitles).then((url) => {
      if (current) artistMotion = { artistId: browseId, url };
    });
    return () => { current = false; };
  });

  $effect(() => {
    const browseId = artistId;
    const allTracksPage = showPopularTracks;
    const accountScope = settingsStore.ytConnected
      ? JSON.stringify([settingsStore.ytAccountEmail, settingsStore.ytAccountName])
      : '';
    remoteSongs = [];
    remoteCatalog = null;
    bannerUrl = '';

    const needsFetch = Boolean(browseId && isTauriRuntime());
    loading = needsFetch;
    if (!needsFetch) return;

    let previousSongs: ArtistPageResponse['songs'];
    let previousShelves: ArtistPageResponse['shelves'];
    return untrack(() => subscribeToArtistPage(browseId, accountScope, (page, complete) => {
      loading = !complete;
      if (!page) return;

      const existingArtist = libraryStore.artists.find((entry) => entry.id === browseId);
      const entry = existingArtist ?? (page.name ? mapYtArtist({ kind: 'artist', name: page.name, browseId }) : undefined);
      if (page.songs !== previousSongs) {
        previousSongs = page.songs;
        const songs = (page.songs ?? []).map(mapYtTrack);
        libraryStore.upsertTracks(songs);
        remoteSongs = songs;
      }
      if (complete && entry && remoteSongs.length) entry.topTrackIds = remoteSongs.map((song) => song.id);
      if (entry && !existingArtist) libraryStore.upsertArtists([entry]);

      if (!allTracksPage) {
        if (page.shelves !== previousShelves || !remoteCatalog) {
          previousShelves = page.shelves;
          const catalog = readArtistReleases(page.shelves ?? [], page.name || entry?.name || '');
          libraryStore.upsertAlbums(catalog.albums);
          remoteCatalog = catalog;
        }
        bannerUrl = page.bannerUrl ?? '';
      }
    }));
  });

  function playPopular(shuffle = false) {
    if (topTracks.length === 0) return;
    const tracks = shuffle ? [...topTracks] : topTracks;
    if (shuffle) {
      for (let i = tracks.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [tracks[i], tracks[j]] = [tracks[j], tracks[i]];
      }
    }
    playTrackNow(tracks[0], tracks);
  }

  function openLatestRelease() {
    if (latestAlbum) goAlbum(latestAlbum.id);
    else if (featuredTrack) playTrackNow(featuredTrack, [featuredTrack]);
  }
</script>

{#snippet releaseTile(album: Album)}
  <button type="button" onclick={() => goAlbum(album.id)} class="release">
    <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" upgrade={false} radius={10} class="aspect-square w-full" />
    <span class="release-title" title={album.title}>{album.title}</span>
    <span class="release-details">{album.year ? album.year + ' · ' : ''}{album.type}</span>
  </button>
{/snippet}

{#snippet artistOverview()}
<div class="artist-page" aria-busy={loading}>
  {#if artist}
    <header class="artist-hero">
      {#if stillUrl}
        <img class="still-backdrop" src={stillUrl} alt="" referrerpolicy="no-referrer" />
        <div class="still-banner">
          <img src={stillUrl} alt="" referrerpolicy="no-referrer" />
        </div>
      {/if}
      {#if motionUrl}
        {#key motionUrl}
          <div class="motion-banner" aria-hidden="true">
            <AnimatedArtworkVideo videoUrls={[motionUrl]} radius={0} class="h-full w-full" />
          </div>
        {/key}
      {/if}

      <div class="artist-identity">
        <h1>{artist.name}</h1>
        {#if artist.monthlyListeners}
          <p>{artist.monthlyListeners}</p>
        {/if}
        <div class="artist-actions">
          <Buttons variant="primary" class="artist-play" disabled={topTracks.length === 0} onclick={() => playPopular()}>
            <WaveIcon name="play" size={15} filled /> Play top tracks
          </Buttons>
          <Buttons class="artist-action" disabled={topTracks.length === 0} onclick={() => playPopular(true)}>
            <WaveIcon name="shuffle" size={14} /> Shuffle
          </Buttons>
          <Buttons variant="toggle" active={Boolean(artist.isFollowing)} class="artist-action" onclick={() => libraryStore.toggleFollowArtist(artistId)}>
            {artist.isFollowing ? 'Following' : 'Follow'}
          </Buttons>
        </div>
      </div>
    </header>
  {:else if loading}
    <div class="artist-hero artist-loading" role="status" aria-label="Loading artist">
      <div class="lw-skeleton h-12 w-60 max-w-full rounded-[8px]"></div>
      <div class="lw-skeleton mt-7 h-9 w-36 rounded-[10px]"></div>
    </div>
  {/if}

  {#if artist || topTracks.length || latestRelease || loading}
    <div class="artist-content">
      <div class="artist-overview" class:artist-split={Boolean((latestRelease || releasesLoading) && (topTracks.length || loading))}>
        {#if latestRelease}
          <section aria-labelledby={id + '-latest-title'}>
            <div class="section-header"><h2 id={id + '-latest-title'}>Latest release</h2></div>
            <button type="button" class="latest-release" onclick={openLatestRelease}>
              <Artwork
                src={latestRelease.artworkUrl}
                label={latestRelease.title}
                artist={latestRelease.artist}
                kind={latestAlbum ? 'album' : 'track'}
                upgrade={false}
                radius={10}
                class="aspect-square w-full"
              />
              <span class="latest-copy">
                <span class="release-details">{latestAlbum?.year ? latestAlbum.year + ' · ' : ''}{latestAlbum?.type ?? 'Song'}</span>
                <span class="latest-name">{latestRelease.title}</span>
                <span class="latest-action">
                  <WaveIcon name={latestAlbum ? 'chevronRight' : 'play'} size={15} filled={!latestAlbum} />
                  {latestAlbum ? 'View release' : 'Play song'}
                </span>
              </span>
            </button>
          </section>
        {:else if releasesLoading}
          <section aria-labelledby={id + '-latest-title'}>
            <div class="section-header"><h2 id={id + '-latest-title'}>Latest release</h2></div>
            <div class="latest-layout" aria-hidden="true">
              <div class="lw-skeleton aspect-square w-full rounded-[10px]"></div>
              <div class="latest-copy">
                <div class="lw-skeleton h-3 w-16 rounded"></div>
                <div class="lw-skeleton h-5 w-full max-w-48 rounded"></div>
              </div>
            </div>
          </section>
        {/if}
        {#if topTracks.length || loading}
          <div class="popular-shelf">
            {#if topTracks.length}
              <ShelfRail
                title="Popular tracks"
                seeAllLabel="Show all"
                onSeeAll={() => goArtistPopularTracks(artistId)}
                preferredCardWidth={394}
                maxVisibleCards={2}
              >
                {#each trackColumns as column (column[0].id)}
                  <div class="artist-tracks">
                    {#each column as track (track.id)}
                      <TrackRow {track} list={topTracks} presentation="artist" showAlbum={false} showQuality={false} />
                    {/each}
                  </div>
                {/each}
              </ShelfRail>
            {:else}
              <ShelfRail title="Popular tracks">
                {#snippet beforeCards()}
                  <Skeletons variant="rows" rows={TRACKS_PER_COLUMN} />
                {/snippet}
              </ShelfRail>
            {/if}
          </div>
        {/if}
      </div>

      {#if essentialAlbums.length}
        <section aria-labelledby={id + '-essentials-title'}>
          <div class="section-header"><h2 id={id + '-essentials-title'}>Essential albums</h2></div>
          <div class="essential-albums">
            {#each essentialAlbums as album (album.id)}
              {@render releaseTile(album)}
            {/each}
          </div>
        </section>
      {/if}

      {#if albums.length}
        <div class="release-shelf">
          <ShelfRail title="Albums">
            {#each albums as album (album.id)}
              {@render releaseTile(album)}
            {/each}
          </ShelfRail>
        </div>
      {/if}

      {#if singles.length}
        <div class="release-shelf">
          <ShelfRail title="Singles & EPs">
            {#each singles as single (single.id)}
              {@render releaseTile(single)}
            {/each}
          </ShelfRail>
        </div>
      {/if}

      {#if !loading && topTracks.length === 0 && !latestRelease}
        <States title="No tracks or releases available" />
      {/if}
    </div>
  {:else}
    <States title="Artist unavailable" description="Open an artist from Search to try again." />
  {/if}
</div>
{/snippet}

{#if showPopularTracks}
  <ArtistPopularTracksPage {artistId} {artist} tracks={topTracks} {loading} />
{:else}
  {@render artistOverview()}
{/if}

<style>
  .artist-page {
    container: artist-page / inline-size;
  }

  .artist-hero {
    position: relative;
    isolation: isolate;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    min-height: clamp(230px, 22cqw, 330px);
    overflow: hidden;
    padding: clamp(24px, 4cqw, 60px);
    background: #101719;
    color: #fff;
  }

  .artist-hero::after {
    position: absolute;
    z-index: -1;
    inset: 0;
    content: '';
    background: linear-gradient(90deg, rgb(8 14 16 / 94%), rgb(8 14 16 / 72%) 34%, transparent 78%),
      linear-gradient(0deg, rgb(8 14 16 / 54%), transparent 54%);
  }

  .still-backdrop,
  .still-banner {
    position: absolute;
    z-index: -2;
    height: 100%;
    pointer-events: none;
  }

  .still-backdrop {
    inset: 0;
    width: 100%;
    filter: blur(36px) brightness(0.65);
    object-fit: cover;
    transform: scale(1.08);
  }

  .still-banner {
    inset: 0 clamp(12px, 5cqw, 100px) 0 auto;
    width: min(58%, 780px);
    display: grid;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    place-items: center;
  }

  .still-banner img {
    display: block;
    max-width: 100%;
    max-height: 100%;
    mask-image: linear-gradient(90deg, transparent, #000 18% 86%, transparent);
  }

  .motion-banner {
    position: absolute;
    z-index: -2;
    inset: 0;
    pointer-events: none;
  }

  .motion-banner :global(video) {
    object-position: center 28%;
    transition: opacity 200ms var(--ease-wave);
  }

  .artist-identity {
    max-width: min(760px, 66%);
  }

  .artist-identity h1 {
    margin: 0;
    font-size: clamp(36px, 5cqw, 82px);
    font-weight: 750;
    line-height: 1.03;
    letter-spacing: -0.035em;
    overflow-wrap: anywhere;
    text-wrap: balance;
  }

  .artist-identity p {
    margin-top: 8px;
    color: #e1e8e9;
    font-size: clamp(14px, 1.25cqw, 19px);
  }

  .artist-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
    margin-top: clamp(16px, 2.1cqw, 26px);
  }

  .artist-actions :global(.artist-play) {
    background: #fff;
    color: #101416;
  }

  .artist-actions :global(button) {
    height: 38px;
    padding-inline: 16px;
    font-size: 14px;
  }

  .artist-actions :global(.artist-action) {
    border-color: rgb(255 255 255 / 52%);
    background: rgb(8 14 16 / 58%);
    color: #fff;
  }

  .artist-actions :global(.artist-action:hover:not(:disabled)) {
    background: rgb(255 255 255 / 18%);
  }

  .artist-actions :global(button:focus-visible) {
    outline: 2px solid #fff;
    outline-offset: 2px;
  }

  .artist-loading {
    align-items: flex-start;
  }

  .artist-loading::after {
    content: none;
  }

  .artist-content {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: clamp(32px, 3.5cqw, 48px);
    padding: 24px clamp(18px, 2.6cqw, 42px) 36px;
  }

  .artist-overview {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
    gap: 28px;
  }

  .artist-overview:empty {
    display: none;
  }

  .section-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    min-height: 32px;
    margin-bottom: 12px;
  }

  .section-header h2,
  .popular-shelf :global(h2),
  .release-shelf :global(h2) {
    margin: 0;
    font-size: clamp(19px, 1.7cqw, 26px);
    font-weight: 700;
    line-height: 1.2;
    letter-spacing: -0.024em;
  }

  .popular-shelf {
    min-width: 0;
  }

  .popular-shelf :global(.shelf-all) {
    color: var(--lw-text-secondary);
  }

  .artist-tracks :global(.track-row) {
    --artist-track-height: clamp(52px, 4.6cqw, 64px);
    position: relative;
  }

  .artist-tracks :global(.track-row + .track-row)::before {
    position: absolute;
    inset: 0 8px auto 60px;
    height: 1px;
    content: '';
    background: var(--lw-outline-soft);
    pointer-events: none;
  }

  .latest-release,
  .latest-layout {
    display: grid;
    grid-template-columns: clamp(100px, 12cqw, 176px) minmax(0, 1fr);
    align-items: center;
    gap: clamp(12px, 1.6cqw, 22px);
    width: 100%;
    max-width: 530px;
    border-radius: 12px;
    padding: 8px;
    background: var(--lw-surface);
  }

  .latest-release {
    text-align: left;
    cursor: pointer;
    transition: background-color var(--lw-motion-fast) var(--ease-native);
  }

  .latest-release:hover {
    background: var(--lw-wash-hover);
  }

  .latest-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding-inline-end: 8px;
  }

  .latest-name {
    color: var(--lw-text-primary);
    font-size: clamp(16px, 1.6cqw, 22px);
    font-weight: 700;
    line-height: 1.22;
    letter-spacing: -0.02em;
    overflow-wrap: anywhere;
  }

  .latest-action {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-top: 7px;
    color: var(--lw-text-secondary);
    font-size: 13px;
  }

  .latest-release:hover .latest-action {
    color: var(--lw-text-primary);
  }

  .essential-albums {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, clamp(130px, 15cqw, 200px)), 1fr));
    gap: 12px;
  }

  .essential-albums .release {
    max-width: 216px;
  }

  .release {
    display: flex;
    min-width: 0;
    flex-direction: column;
    border-radius: 9px;
    text-align: left;
    cursor: pointer;
  }

  .release-title,
  .release-details {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .release-title {
    margin-top: 7px;
    color: var(--lw-text-primary);
    font-size: 15px;
    font-weight: 650;
  }

  .release:hover .release-title {
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .release-details {
    color: var(--lw-text-secondary);
    font-size: 13px;
  }

  @container artist-page (max-width: 849px) {
    .artist-identity {
      max-width: min(540px, 92%);
    }
  }

  @container artist-page (min-width: 850px) {
    .artist-split {
      grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
      column-gap: clamp(28px, 3.3cqw, 56px);
    }
  }

</style>
