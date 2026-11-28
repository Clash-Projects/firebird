<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { HomeEntityItem } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import ShelfRail from '$src/ui/components/ShelfRail.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { goAlbum, goArtist, goPlaylist } from '$src/ui/navigation/waveNav';
  import WaveEntrance from '$src/ui/theme/WaveEntrance.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import HomeSpotlight from './HomeSpotlight.svelte';
  import SpotifyHome from './SpotifyHome.svelte';

  const today = new Date();
  const hour = today.getHours();
  const greeting =
    hour < 5 ? 'Up late' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
  const dateLabel = new Intl.DateTimeFormat('en-US', {
    weekday: 'long',
    month: 'short',
    day: 'numeric',
  }).format(today);

  let username = $derived(
    settingsStore.displayName.trim() || settingsStore.ytAccountName.trim()
  );
  let shelves = $derived(libraryStore.homeShelves);
  let firstTrackShelf = $derived(
    shelves.find((shelf) => shelf.tracks.length || shelf.trackCards.length)
  );
  let heroPool = $derived(
    firstTrackShelf
      ? firstTrackShelf.tracks.length
        ? firstTrackShelf.tracks
        : firstTrackShelf.trackCards
      : libraryStore.tracks
  );
  let featured = $derived(heroPool[0]);
  let heroKicker = $derived(
    settingsStore.ytConnected ? 'FOR YOU · YOUTUBE MUSIC MIX' : 'FOR YOU · FEATURED MIX'
  );
  let quickPicks = $derived(libraryStore.tracks.slice(0, 9));
  let charts = $derived(libraryStore.tracks.slice(0, 5));
  let visibleIds = $derived(
    new Set(settingsStore.homeShelves.filter((s) => s.visible).map((s) => s.id))
  );

  function openEntity(e: HomeEntityItem) {
    if (e.kind === 'artist') {
      if (e.browseId) goArtist(e.browseId);
      return;
    }
    if (e.kind === 'playlist' || e.kind === 'mix') {
      const pid = e.playlistId || e.browseId;
      if (pid) goPlaylist(pid, 'youtube');
      return;
    }
    if (e.browseId) goAlbum(e.browseId);
    else if (e.playlistId) goPlaylist(e.playlistId, 'youtube');
  }
</script>

<div class="home-page" data-lw="home-page">
  <WaveEntrance index={0}>
    <header class="flex items-center justify-between gap-4">
      <div class="min-w-0">
        <h1 class="truncate text-[22px] leading-[1.2] font-bold tracking-[-0.4px]">
          {username ? `${greeting}, ${username}` : greeting}
        </h1>
        <p class="mt-0.5 text-[13px] text-[var(--lw-text-secondary)]">
          {username ? `For ${username} · ${dateLabel} · picked from your taste` : `${dateLabel} · picked for you`}
        </p>
      </div>
      <button
        type="button"
        class="inline-flex h-8 shrink-0 cursor-pointer items-center gap-1.5 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-3 text-[12px] font-semibold text-[var(--lw-text-secondary)] transition-colors hover:bg-[var(--lw-surface-raised)] hover:text-[var(--lw-text-primary)]"
        onclick={() => router.navigate('/home/layout')}
      >
        <WaveIcon name="wand" size={13} />
        Edit home
      </button>
    </header>
  </WaveEntrance>

  <WaveEntrance index={1}>
    <HomeSpotlight track={featured} queue={heroPool} kicker={heroKicker} />
  </WaveEntrance>

  {#if !featured && libraryStore.loadingHome}
    <WaveEntrance index={2}>
      <section class="space-y-3" aria-busy="true">
        <div class="text-[12px] text-[var(--lw-text-secondary)]">Loading YouTube Music…</div>
        <div class="quick-picks-grid">
          {#each Array(6) as _}
            <div class="lw-skeleton h-[56px] rounded-[10px]"></div>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {:else if !featured && shelves.length === 0 && libraryStore.albums.length === 0 && libraryStore.artists.length === 0}
    <WaveEntrance index={2}>
      <section class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 class="text-[16px] font-bold">Discover</h2>
            <p class="mt-1 text-[13px] text-[var(--lw-text-secondary)]">
              {libraryStore.homeError ?? 'Connect YouTube Music to see personalized recommendations from your account.'}
            </p>
          </div>
          <div class="flex items-center gap-2">
            <button
              type="button"
              onclick={() => void libraryStore.loadHomeFeed(settingsStore.ytConnected, true)}
              class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3 text-[12px] font-semibold"
            >
              <WaveIcon name="refresh" size={13} /> Refresh
            </button>
            <button
              type="button"
              onclick={() => router.navigate('/settings/integrations')}
              class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] bg-[var(--lw-accent)] px-3 text-[12px] font-bold text-[var(--lw-on-accent)]"
            >
              <WaveIcon name="globe" size={13} /> Music accounts
            </button>
          </div>
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if shelves.length > 0}
    {#each shelves as shelf, sIdx (`${shelf.title}-${sIdx}`)}
      {@const playList = shelf.tracks.length > 0 ? shelf.tracks : shelf.trackCards}
      {#snippet playAll()}
        <button
          type="button"
          onclick={() => playList[0] && playTrackNow(playList[0], playList)}
          class="cursor-pointer rounded-[6px] px-2 py-1 text-[12px] font-medium text-[var(--lw-accent)] hover:underline"
        >
          Play all
        </button>
      {/snippet}

      {#snippet trackList()}
        {#if shelf.tracks.length > 0}
          {@const gridCount = shelf.tracks.length >= 9 ? 9 : shelf.tracks.length >= 6 ? 6 : shelf.tracks.length}
          <div class="quick-picks-grid">
            {#each shelf.tracks.slice(0, gridCount) as track (`${shelf.title}-${track.id}`)}
              {@const playing = playerStore.currentTrack?.id === track.id}
              <button
                type="button"
                onclick={() => playTrackNow(track, shelf.tracks)}
                oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
                class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-1.5 py-1 text-left transition-colors {playing
                  ? 'bg-[var(--lw-accent-subtle)]'
                  : 'hover:bg-[var(--lw-wash-hover)]'}"
              >
                <div class="relative h-[44px] w-[44px] shrink-0 overflow-hidden rounded-[8px]">
                  <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={44} radius={8} />
                  <div
                    class="absolute inset-0 flex items-center justify-center bg-black/55 text-white transition-opacity {playing
                      ? 'opacity-100'
                      : 'opacity-0 group-hover:opacity-100'}"
                  >
                    <WaveIcon name={playing && playerStore.isPlaying ? 'pause' : 'play'} size={15} filled />
                  </div>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-semibold {playing ? 'text-[var(--lw-accent)]' : ''}">
                    {track.title}
                  </div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
                </div>
                {#if track.durationSec > 0}
                  <span class="pr-2 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
                    {formatDuration(track.durationSec)}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      {/snippet}

      {#snippet cards()}
        {#each shelf.trackCards as track (`card-${shelf.title}-${track.id}`)}
          <button
            type="button"
            onclick={() => playTrackNow(track, shelf.trackCards)}
            oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
            class="group min-w-0 cursor-pointer text-left"
          >
            <div class="relative aspect-square overflow-hidden rounded-[12px] transition-all duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-hover:shadow-[var(--lw-shadow-lift)] group-active:scale-[0.97]">
              <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} class="h-full w-full" radius={12} />
              <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity duration-[110ms] group-hover:opacity-100">
                <div class="flex h-11 w-11 scale-75 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[0_4px_12px_rgba(0,0,0,0.4)] transition-transform duration-[110ms] group-hover:scale-100">
                  <WaveIcon name="play" size={18} filled />
                </div>
              </div>
            </div>
            <div class="mt-2 w-0 min-w-full truncate text-[13px] font-semibold">{track.title}</div>
            <div class="w-0 min-w-full truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
          </button>
        {/each}

        {#each shelf.entities as entity (`ent-${shelf.title}-${entity.id}`)}
          {#if entity.kind === 'artist'}
            <button
              type="button"
              onclick={() => openEntity(entity)}
              class="group min-w-0 cursor-pointer text-center"
            >
              <div class="mx-auto aspect-square w-full max-w-[124px] transition-transform duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-active:scale-[0.97]">
                <Artwork src={entity.artworkUrl} label={entity.name} kind="artist" alt={entity.name} class="h-full w-full" circle shadow="lift" />
              </div>
              <div class="mt-2.5 truncate text-[13px] font-semibold">{entity.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{entity.subtitle || 'Artist'}</div>
            </button>
          {:else}
            <button
              type="button"
              onclick={() => openEntity(entity)}
              class="group min-w-0 cursor-pointer text-left"
            >
              <div class="relative aspect-square overflow-hidden rounded-[12px] transition-all duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-hover:shadow-[var(--lw-shadow-lift)] group-active:scale-[0.97]">
                <Artwork
                  src={entity.artworkUrl}
                  label={entity.name}
                  artist={entity.subtitle}
                  kind="album"
                  upgrade={!entity.artworkUrl && entity.kind === 'album'}
                  alt={entity.name}
                  class="h-full w-full"
                  radius={12}
                />
                <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity duration-[110ms] group-hover:opacity-100">
                  <div class="flex h-11 w-11 scale-75 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[0_4px_12px_rgba(0,0,0,0.4)] transition-transform duration-[110ms] group-hover:scale-100">
                    <WaveIcon name="play" size={18} filled />
                  </div>
                </div>
              </div>
              <div class="mt-2 truncate text-[13px] font-semibold">{entity.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{entity.subtitle}</div>
            </button>
          {/if}
        {/each}
      {/snippet}

      <WaveEntrance index={Math.min(sIdx + 2, 6)}>
        <ShelfRail
          title={shelf.title}
          eyebrow={shelf.subtitle}
          headerAction={playList.length > 0 ? playAll : undefined}
          beforeCards={shelf.tracks.length > 0 ? trackList : undefined}
          children={shelf.trackCards.length > 0 || shelf.entities.length > 0 ? cards : undefined}
        />
      </WaveEntrance>
    {/each}
  {/if}

  {#if !shelves.length && visibleIds.has('quick_picks') && quickPicks.length > 0}
    <WaveEntrance index={3}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <h2 class="text-[15.5px] font-semibold tracking-[-0.1px]">Quick Picks</h2>
          <button
            type="button"
            onclick={() => quickPicks[0] && playTrackNow(quickPicks[0], quickPicks)}
            class="cursor-pointer rounded-[6px] px-2 py-1 text-[12px] font-medium text-[var(--lw-accent)] hover:underline"
          >
            Play all
          </button>
        </div>

        <div class="quick-picks-grid">
          {#each quickPicks as track (track.id)}
            {@const playing = playerStore.currentTrack?.id === track.id}
            <button
              type="button"
              onclick={() => playTrackNow(track, quickPicks)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-1.5 py-1 text-left transition-colors {playing
                ? 'bg-[var(--lw-accent-subtle)]'
                : 'hover:bg-[var(--lw-wash-hover)]'}"
            >
              <div class="relative h-[44px] w-[44px] shrink-0 overflow-hidden rounded-[8px]">
                <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" alt={track.title} size={44} radius={8} />
                <div
                  class="absolute inset-0 flex items-center justify-center bg-black/55 text-white transition-opacity {playing
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100'}"
                >
                  <WaveIcon name={playing && playerStore.isPlaying ? 'pause' : 'play'} size={15} filled />
                </div>
              </div>
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13px] font-semibold {playing ? 'text-[var(--lw-accent)]' : ''}">
                  {track.title}
                </div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
              {#if track.durationSec > 0}
                <span class="pr-2 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
                  {formatDuration(track.durationSec)}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if !shelves.length && visibleIds.has('albums') && libraryStore.albums.length > 0}
    <WaveEntrance index={4}>
      <ShelfRail title="Albums For You" eyebrow="Rotation" onSeeAll={() => router.navigate('/albums')}>
        {#each libraryStore.albums as album (album.id)}
          <button
            type="button"
            onclick={() => goAlbum(album.id)}
            class="lw-press group min-w-0 cursor-pointer text-left"
          >
            <div class="relative aspect-square overflow-hidden rounded-[12px]">
              <Artwork src={album.artworkUrl} alt={album.title} class="h-full w-full" radius={12} />
              <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity group-hover:opacity-100">
                <div class="flex h-11 w-11 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[0_4px_12px_rgba(0,0,0,0.4)]">
                  <WaveIcon name="arrowRight" size={18} />
                </div>
              </div>
            </div>
            <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist} · {album.year}</div>
          </button>
        {/each}
      </ShelfRail>
    </WaveEntrance>
  {/if}

  {#if !shelves.length && visibleIds.has('personal_mix') && libraryStore.artists.length > 0}
    <WaveEntrance index={5}>
      <ShelfRail title="Artists For You" onSeeAll={() => router.navigate('/artists')}>
        {#each libraryStore.artists as artist (artist.id)}
          <button
            type="button"
            onclick={() => goArtist(artist.id)}
            class="lw-press group min-w-0 cursor-pointer text-center"
          >
            <div class="mx-auto aspect-square w-full max-w-[124px]">
              <Artwork src={artist.artworkUrl} alt={artist.name} class="h-full w-full" circle shadow="lift" />
            </div>
            <div class="mt-2.5 truncate text-[13px] font-semibold">{artist.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{artist.monthlyListeners}</div>
          </button>
        {/each}
      </ShelfRail>
    </WaveEntrance>
  {/if}

  {#if !shelves.length && visibleIds.has('recent_plays') && charts.length > 0}
    <WaveEntrance index={6}>
      <section class="space-y-2">
        <div>
          <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">CHARTS</div>
          <h2 class="text-[15.5px] font-semibold tracking-[-0.1px]">Trending Now</h2>
        </div>

        <div class="space-y-0.5">
          {#each charts as track, i (track.id)}
            <button
              type="button"
              onclick={() => playTrackNow(track, charts)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[54px] w-full cursor-pointer items-center gap-3.5 rounded-[10px] px-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
            >
              <span class="w-8 text-center text-[20px] font-bold tracking-[-0.5px] text-[var(--lw-text-tertiary)] lw-tabular">
                {String(i + 1).padStart(2, '0')}
              </span>
              <Artwork src={track.artworkUrl} alt={track.title} size={40} radius={8} />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13.5px] font-semibold">{track.title}</div>
                <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
              <span class="w-11 text-right text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">
                {formatDuration(track.durationSec)}
              </span>
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if visibleIds.has('spotify_home') && settingsStore.spotifyConnected}
    <SpotifyHome />
  {/if}
</div>

<style>
  .home-page {
    display: grid;
    max-width: 1440px;
    grid-template-columns: minmax(0, 1fr);
    grid-auto-rows: max-content;
    gap: 22px;
    margin-inline: auto;
    padding: 20px clamp(18px, 2.5vw, 32px) 44px;
    container-type: inline-size;
  }

  .quick-picks-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 4px 10px;
  }

  @container (max-width: 820px) {
    .quick-picks-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @container (max-width: 480px) {
    .quick-picks-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
