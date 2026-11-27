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
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import HomeSpotlight from './HomeSpotlight.svelte';
  import SpotifyHome from './SpotifyHome.svelte';

  const today = new Date();
  const hour = today.getHours();
  const greeting = hour < 5 ? 'Up late' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
  const dateLabel = new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(today);
  const shortcuts: ReadonlyArray<{ label: string; description: string; icon: WaveIconName; route: string }> = [
    { label: 'Liked songs', description: 'Your favorites', icon: 'heart', route: '/liked' },
    { label: 'Playlists', description: 'Your mixes', icon: 'queue', route: '/playlists' },
    { label: 'Downloads', description: 'Listen offline', icon: 'download', route: '/downloads' },
    { label: 'History', description: 'Recently played', icon: 'clock', route: '/history' },
  ];

  let username = $derived(settingsStore.ytAccountName);
  let shelves = $derived(libraryStore.homeShelves);
  let firstTrackShelf = $derived(shelves.find((shelf) => shelf.tracks.length || shelf.trackCards.length));
  let heroPool = $derived(firstTrackShelf
    ? (firstTrackShelf.tracks.length ? firstTrackShelf.tracks : firstTrackShelf.trackCards)
    : libraryStore.tracks);
  let featured = $derived(heroPool[0]);
  let quickPicks = $derived(libraryStore.tracks.slice(0, 6));
  let charts = $derived(libraryStore.tracks.slice(0, 5));
  let visibleIds = $derived(new Set(settingsStore.homeShelves.filter((s) => s.visible).map((s) => s.id)));

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
    <header class="home-header">
      <div class="min-w-0">
        <h1 class="lw-display home-title" title={username ? `${greeting}, ${username}` : greeting}>
          {username ? `${greeting}, ${username}` : greeting}
        </h1>
        <p>{dateLabel}</p>
      </div>
      <button type="button" class="home-edit" onclick={() => router.navigate('/home/layout')}>
        <WaveIcon name="layers" size={15} /> Customize home
      </button>
    </header>
  </WaveEntrance>

  <WaveEntrance index={1}>
    <HomeSpotlight track={featured} queue={heroPool} />
  </WaveEntrance>

  <WaveEntrance index={2}>
    <section aria-labelledby="home-shortcuts">
      <h2 class="home-section-title lw-display" id="home-shortcuts">Your collection</h2>
      <div class="home-shortcuts">
        {#each shortcuts as shortcut (shortcut.route)}
          <button type="button" class="home-shortcut" onclick={() => router.navigate(shortcut.route)}>
            <span class="shortcut-icon"><WaveIcon name={shortcut.icon} size={19} /></span>
            <span class="min-w-0 flex-1">
              <span class="shortcut-title">{shortcut.label}</span>
              <span class="shortcut-description">{shortcut.description}</span>
            </span>
            <WaveIcon class="shortcut-arrow" name="chevronRight" size={15} />
          </button>
        {/each}
      </div>
    </section>
  </WaveEntrance>

  {#if !featured && (libraryStore.loadingHome || (libraryStore.albums.length === 0 && libraryStore.artists.length === 0))}
    <WaveEntrance index={3}>
      <section class="home-feed" aria-labelledby="home-discovery" aria-busy={libraryStore.loadingHome}>
        <div class="flex items-center justify-between gap-3">
          <h2 class="home-section-title lw-display" id="home-discovery">Discover</h2>
          <button
            type="button"
            class="home-edit feed-refresh"
            onclick={() => void libraryStore.loadHomeFeed(settingsStore.ytConnected, true)}
            disabled={libraryStore.loadingHome}
          >
            <WaveIcon name="refresh" size={14} /> Refresh feed
          </button>
        </div>
        {#if libraryStore.loadingHome}
          <p role="status">Loading YouTube Music…</p>
          <div class="feed-skeleton" aria-hidden="true">
            {#each Array(3) as _}
              <div class="lw-skeleton h-[50px] rounded-[9px]"></div>
            {/each}
          </div>
        {:else}
          <p>{libraryStore.homeError ?? 'Connect YouTube Music to see recommendations from your account.'}</p>
          <button type="button" class="feed-connect" onclick={() => router.navigate('/settings/integrations')}>
            <WaveIcon name="globe" size={15} /> Music accounts <WaveIcon name="arrowRight" size={14} />
          </button>
        {/if}
      </section>
    </WaveEntrance>
  {/if}

  {#if shelves.length > 0}
    {#each shelves as shelf, sIdx (`${shelf.title}-${sIdx}`)}
      <WaveEntrance index={Math.min(sIdx + 2, 6)}>
        <section class="space-y-2.5">
          <div class="flex items-end justify-between gap-2">
            <div>
              {#if shelf.subtitle}
                <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
                  {shelf.subtitle}
                </div>
              {/if}
              <h2 class="home-section-title lw-display">{shelf.title}</h2>
            </div>

            {#if shelf.tracks.length > 0 || shelf.trackCards.length > 0}
              {@const list = shelf.tracks.length > 0 ? shelf.tracks : shelf.trackCards}
              <button
                type="button"
                onclick={() => list[0] && playTrackNow(list[0], list)}
                class="cursor-pointer rounded-[8px] px-2.5 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
              >
                Play all
              </button>
            {/if}
          </div>

          {#if shelf.tracks.length > 0}
            <div class="quick-picks-grid">
              {#each shelf.tracks.slice(0, 12) as track (`${shelf.title}-${track.id}`)}
                {@const playing = playerStore.currentTrack?.id === track.id}
                <button
                  type="button"
                  onclick={() => playTrackNow(track, shelf.tracks)}
                  oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
                  class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[12px] px-1.5 py-1 text-left transition-colors {playing
                    ? 'bg-[var(--lw-accent)]/12'
                    : 'hover:bg-white/[0.05]'}"
                >
                  <div class="relative h-[44px] w-[44px] shrink-0 overflow-hidden rounded-[10px]">
                    <Artwork src={track.artworkUrl} alt={track.title} size={44} radius={10} />
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

          {#if shelf.trackCards.length > 0}
            <div class="lw-scroll-x flex gap-3 py-1.5">
              {#each shelf.trackCards as track (`card-${shelf.title}-${track.id}`)}
                <button
                  type="button"
                  onclick={() => playTrackNow(track, shelf.trackCards)}
                  oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
                  class="group w-[160px] shrink-0 cursor-pointer text-left"
                >
                  <div class="relative overflow-hidden rounded-[12px] transition-all duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-hover:shadow-[var(--lw-shadow-lift)] group-active:scale-[0.97]">
                    <Artwork src={track.artworkUrl} alt={track.title} size={160} radius={12} />
                    <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity duration-[110ms] group-hover:opacity-100">
                      <div class="flex h-11 w-11 scale-75 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[0_4px_12px_rgba(0,0,0,0.4)] transition-transform duration-[110ms] group-hover:scale-100">
                        <WaveIcon name="play" size={18} filled />
                      </div>
                    </div>
                  </div>
                  <div class="mt-2 truncate text-[13px] font-semibold">{track.title}</div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
                </button>
              {/each}
            </div>
          {/if}

          {#if shelf.entities.length > 0}
            <div class="lw-scroll-x flex gap-3.5 py-1.5">
              {#each shelf.entities as entity (`ent-${shelf.title}-${entity.id}`)}
                {#if entity.kind === 'artist'}
                  <button
                    type="button"
                    onclick={() => openEntity(entity)}
                    class="group w-[144px] shrink-0 cursor-pointer text-center"
                  >
                    <div class="mx-auto w-[124px] transition-transform duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-active:scale-[0.97]">
                      <Artwork src={entity.artworkUrl} alt={entity.name} size={124} circle shadow="lift" />
                    </div>
                    <div class="mt-2.5 truncate text-[13px] font-semibold">{entity.name}</div>
                    <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{entity.subtitle || 'Artist'}</div>
                  </button>
                {:else}
                  <button
                    type="button"
                    onclick={() => openEntity(entity)}
                    class="group w-[160px] shrink-0 cursor-pointer text-left"
                  >
                    <div class="relative overflow-hidden rounded-[12px] transition-all duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-hover:shadow-[var(--lw-shadow-lift)] group-active:scale-[0.97]">
                      <Artwork src={entity.artworkUrl} alt={entity.name} size={160} radius={12} />
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
            </div>
          {/if}
        </section>
      </WaveEntrance>
    {/each}
  {/if}

  {#if !shelves.length && visibleIds.has('quick_picks') && quickPicks.length > 0}
    <WaveEntrance index={3}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <h2 class="home-section-title lw-display">Quick Picks</h2>
          <button
            type="button"
            onclick={() => quickPicks[0] && playTrackNow(quickPicks[0], quickPicks)}
            class="cursor-pointer rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
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
              class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[12px] px-1 py-1 text-left transition-colors {playing
                ? 'bg-[var(--lw-accent)]/12'
                : 'hover:bg-[var(--lw-wash-hover)]'}"
            >
              <div class="relative h-[48px] w-[48px] shrink-0 overflow-hidden rounded-[12px]">
                <Artwork src={track.artworkUrl} alt={track.title} size={48} radius={12} />
                <div
                  class="absolute inset-0 flex items-center justify-center bg-black/55 text-white transition-opacity {playing
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100'}"
                >
                  <WaveIcon name={playing && playerStore.isPlaying ? 'pause' : 'play'} size={16} filled />
                </div>
              </div>
              <div class="min-w-0 flex-1">
                <div class="truncate text-[12.5px] font-semibold {playing ? 'text-[var(--lw-accent)]' : ''}">
                  {track.title}
                </div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
              {#if track.durationSec > 0}
                <span class="pr-2 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">{formatDuration(track.durationSec)}</span>
              {/if}
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if !shelves.length && visibleIds.has('albums') && libraryStore.albums.length > 0}
    <WaveEntrance index={3}>
      <ShelfRail title="Albums For You" eyebrow="Rotation" onSeeAll={() => router.navigate('/albums')}>
          {#each libraryStore.albums as album (album.id)}
            <button
              type="button"
              onclick={() => goAlbum(album.id)}
              class="lw-press group w-[160px] shrink-0 cursor-pointer text-left"
            >
              <div class="relative overflow-hidden rounded-[12px]">
                <Artwork src={album.artworkUrl} alt={album.title} size={160} radius={12} />
                <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
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
    <WaveEntrance index={4}>
      <ShelfRail title="Artists For You" onSeeAll={() => router.navigate('/artists')}>
          {#each libraryStore.artists as artist (artist.id)}
            <button
              type="button"
              onclick={() => goArtist(artist.id)}
              class="lw-press group w-[144px] shrink-0 cursor-pointer text-center"
            >
              <div class="mx-auto w-[124px]">
                <Artwork src={artist.artworkUrl} alt={artist.name} size={124} circle shadow="lift" />
              </div>
              <div class="mt-2.5 truncate text-[13px] font-semibold">{artist.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{artist.monthlyListeners}</div>
            </button>
          {/each}
      </ShelfRail>
    </WaveEntrance>
  {/if}

  {#if !shelves.length && visibleIds.has('recent_plays') && charts.length > 0}
    <WaveEntrance index={5}>
      <section class="space-y-2">
        <div>
          <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">CHARTS</div>
          <h2 class="home-section-title lw-display">Trending Now</h2>
        </div>

        <div class="space-y-0.5">
          {#each charts as track, i (track.id)}
            <button
              type="button"
              onclick={() => playTrackNow(track, charts)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[54px] w-full cursor-pointer items-center gap-3.5 rounded-[12px] px-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
            >
              <span class="w-8 text-center text-[20px] font-bold tracking-[-0.5px] text-[var(--lw-text-tertiary)] lw-tabular">
                {String(i + 1).padStart(2, '0')}
              </span>
              <Artwork src={track.artworkUrl} alt={track.title} size={40} radius={10} />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13.5px] font-semibold">{track.title}</div>
                <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
              <span class="hidden rounded-[6px] border border-[var(--lw-outline)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-tertiary)] sm:inline">
                {track.codec}
              </span>
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
    gap: 24px;
    margin-inline: auto;
    padding: 23px clamp(18px, 2.6vw, 38px) 44px;
    container-type: inline-size;
  }

  .home-header { display: flex; align-items: center; justify-content: space-between; gap: 14px; }
  .home-title {
    overflow: hidden;
    font-size: clamp(27px, 3.1cqi, 39px);
    font-weight: 700;
    line-height: 1.12;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .home-header p { margin-top: 3px; color: var(--lw-text-secondary); font-size: 13px; }
  .home-edit {
    display: inline-flex;
    min-height: 34px;
    flex: none;
    align-items: center;
    gap: 7px;
    padding-inline: 10px;
    border: 1px solid var(--lw-outline);
    border-radius: 8px;
    background: var(--lw-surface);
    color: var(--lw-text-secondary);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast);
  }

  .home-section-title { font-size: 22px; font-weight: 700; line-height: 1.25; }
  .quick-picks-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 4px 8px; }
  .home-shortcuts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    margin-top: 10px;
  }

  .home-shortcut {
    display: flex;
    min-height: 67px;
    align-items: center;
    gap: 11px;
    padding: 9px 11px;
    border: 1px solid var(--lw-outline-soft);
    border-radius: 12px;
    background: var(--lw-surface);
    cursor: pointer;
    text-align: left;
    transition: background-color var(--lw-motion-fast), border-color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }

  .home-shortcut:active { transform: scale(calc(1 - 0.02 * var(--lw-motion-factor))); }
  .shortcut-icon {
    display: grid;
    width: 37px;
    height: 37px;
    flex: none;
    place-items: center;
    border: 1px solid color-mix(in srgb, var(--lw-accent) 18%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--lw-accent) 9%, transparent);
    color: var(--lw-accent);
  }
  .shortcut-title, .shortcut-description { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .shortcut-title { font-size: 13px; font-weight: 700; }
  .shortcut-description { color: var(--lw-text-secondary); font-size: 11.5px; }
  .home-shortcut :global(.shortcut-arrow) {
    color: var(--lw-text-tertiary);
    transition: color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }

  .home-feed { border-top: 1px solid var(--lw-divider); padding-top: 15px; }
  .home-feed p { margin-top: 4px; color: var(--lw-text-secondary); font-size: 13px; }
  .feed-refresh:disabled { cursor: default; opacity: 0.55; }
  .feed-connect { display: inline-flex; align-items: center; gap: 7px; margin-top: 9px; color: var(--lw-accent); cursor: pointer; font-weight: 650; }
  .feed-skeleton { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; margin-top: 10px; }

  @media (hover: hover) and (pointer: fine) {
    .home-edit:not(:disabled):hover { background: var(--lw-surface-raised); color: var(--lw-text-primary); }
    .home-shortcut:hover { border-color: var(--lw-outline); background: var(--lw-surface-raised); }
    .home-shortcut:hover :global(.shortcut-arrow) { color: var(--lw-accent); transform: translateX(calc(2px * var(--lw-motion-factor))); }
    .feed-connect:hover { text-decoration: underline; text-underline-offset: 3px; }
  }

  @container (max-width: 790px) {
    .quick-picks-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .home-shortcut { gap: 8px; padding-inline: 8px; }
    .home-shortcut :global(.shortcut-arrow) { display: none; }
  }

  @container (max-width: 640px) {
    .home-shortcuts { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }

  @container (max-width: 460px) {
    .quick-picks-grid { grid-template-columns: minmax(0, 1fr); }
  }
</style>
