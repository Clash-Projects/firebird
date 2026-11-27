<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { HomeEntityItem } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goAlbum, goArtist, goPlaylist } from '$src/ui/navigation/waveNav';
  import WaveEntrance from '$src/ui/theme/WaveEntrance.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import SpotifyHome from './SpotifyHome.svelte';

  const greeting = (() => {
    const h = new Date().getHours();
    if (h < 5) return 'Up late';
    if (h < 12) return 'Good morning';
    if (h < 18) return 'Good afternoon';
    return 'Good evening';
  })();

  const dateLabel = new Intl.DateTimeFormat('en-US', {
    weekday: 'long',
    month: 'short',
    day: 'numeric',
  }).format(new Date());

  let username = $derived(settingsStore.ytAccountName);
  let shelves = $derived(libraryStore.homeShelves);
  let firstTrackShelf = $derived(shelves.find((s) => s.tracks.length > 0 || s.trackCards.length > 0));
  let heroPool = $derived(
    firstTrackShelf
      ? (firstTrackShelf.tracks.length > 0 ? firstTrackShelf.tracks : firstTrackShelf.trackCards)
      : libraryStore.tracks
  );
  let featured = $derived(heroPool[0]);
  let companions = $derived(heroPool.slice(1, 4));
  let fallbackQuickPicks = $derived(libraryStore.tracks.slice(0, 6));
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

<div class="w-full space-y-7 px-4 pt-[22px] pb-12 md:px-6 xl:px-7">
  <WaveEntrance index={0}>
    <header class="flex items-center justify-between gap-4">
      <div class="min-w-0">
        <h1 class="truncate text-[24px] font-bold tracking-[-0.025em] leading-[1.15]">
          {username ? `${greeting}, ${username}` : greeting}
        </h1>
        <p class="mt-0.5 text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.ytConnected && username
            ? `YouTube Music · ${username} · ${dateLabel}`
            : settingsStore.ytConnected
              ? `YouTube Music · ${dateLabel}`
              : `${dateLabel} · picked for you`}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          onclick={() => void libraryStore.loadHomeFeed(settingsStore.ytConnected, true)}
          disabled={libraryStore.loadingHome}
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[10px] px-3 text-[12px] font-semibold text-[var(--lw-text-secondary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] disabled:opacity-50"
        >
          <WaveIcon name="refresh" size={14} class={libraryStore.loadingHome ? 'animate-spin' : ''} />
          Refresh
        </button>
        <button
          type="button"
          onclick={() => router.navigate('/home/layout')}
          class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[10px] px-3 text-[12px] font-semibold text-[var(--lw-text-secondary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="layers" size={14} />
          Edit home
        </button>
      </div>
    </header>
  </WaveEntrance>

  {#if libraryStore.loadingHome && shelves.length === 0 && !featured}
    <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-6 py-12 text-center text-[13px] text-[var(--lw-text-secondary)]">
      Loading YouTube Music feed...
    </div>
  {:else if shelves.length === 0 && !featured && libraryStore.albums.length === 0 && libraryStore.artists.length === 0}
    <div class="space-y-3 text-center">
      <States
        icon="music"
        title="Your feed is empty"
        description={libraryStore.homeError ?? 'Sign in to YouTube Music or add local music folders in Settings to start listening.'}
      />
      <button
        type="button"
        onclick={() => void libraryStore.loadHomeFeed(settingsStore.ytConnected, true)}
        class="inline-flex cursor-pointer items-center gap-2 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-4 py-2 text-[12.5px] font-semibold text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
      >
        Refresh Feed
      </button>
    </div>
  {/if}

  {#if featured}
    <WaveEntrance index={1}>
      <section class="lw-surface-lit relative overflow-hidden rounded-[20px] bg-white/[0.035] p-5">
        <div class="flex flex-col gap-5 lg:flex-row lg:items-center">
          <div class="flex min-w-0 flex-[58] items-center gap-5">
            <div class="relative h-[132px] w-[132px] shrink-0 sm:h-[176px] sm:w-[176px]">
              <div
                class="pointer-events-none absolute -inset-[75px] rounded-full opacity-90 transition-colors duration-500"
                style:background="radial-gradient(circle, color-mix(in srgb, {featured.accentColor} 34%, transparent) 0%, color-mix(in srgb, {featured.accentColor} 12%, transparent) 55%, transparent 100%)"
              ></div>
              <div class="relative z-10 h-full w-full">
                <Artwork src={featured.artworkUrl} alt={featured.title} radius={12} shadow="artwork" class="h-full w-full" />
              </div>
            </div>

            <div class="relative z-10 min-w-0 flex-1">
              <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
                {firstTrackShelf?.title || 'FOR YOU · FEATURED MIX'}
              </div>
              <h2 class="mt-1.5 line-clamp-2 text-[22px] font-bold tracking-[-0.5px] leading-[1.15] sm:text-[26px]">
                {featured.title}
              </h2>
              <p class="mt-1 truncate text-[14px] text-[var(--lw-text-secondary)]">{featured.artist}</p>
              <div class="mt-0.5 text-[12px] text-[var(--lw-text-tertiary)]">
                {featured.codec} · {featured.bitDepth}-bit / {(featured.sampleRateHz / 1000).toFixed(0)} kHz
              </div>

              <div class="mt-3.5 flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  onclick={() => playTrackNow(featured, heroPool)}
                  class="inline-flex cursor-pointer items-center gap-2 rounded-full bg-[var(--lw-accent)] px-[22px] py-[10px] text-[12px] font-semibold tracking-[0.2px] text-[var(--lw-on-accent)] transition-all hover:brightness-95 active:scale-[0.98]"
                >
                  <WaveIcon name="play" size={14} filled />
                  Play
                </button>
                <button
                  type="button"
                  onclick={() => {
                    playTrackNow(featured, [...heroPool].reverse());
                    featuresStore.showToast({ title: 'Started endless radio', subtitle: featured.title, severity: 'info' });
                  }}
                  class="inline-flex cursor-pointer items-center gap-2 rounded-full border border-[var(--lw-divider)] px-[22px] py-[10px] text-[12px] font-semibold tracking-[0.2px] text-[var(--lw-text-secondary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
                >
                  <WaveIcon name="radio" size={14} />
                  Radio
                </button>
              </div>
            </div>
          </div>

          <div class="hidden h-[180px] w-px bg-white/[0.08] lg:block"></div>

          <div class="relative z-10 min-w-0 flex-[42]">
            <div class="mb-2 text-[9.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
              MORE FOR YOU
            </div>
            <div class="space-y-0.5">
              {#each companions as track (track.id)}
                <button
                  type="button"
                  onclick={() => playTrackNow(track, heroPool)}
                  oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
                  class="group flex w-full cursor-pointer items-center gap-2.5 rounded-[12px] px-1.5 py-[5px] text-left transition-colors hover:bg-white/[0.05]"
                >
                  <div class="relative h-[44px] w-[44px] shrink-0 overflow-hidden rounded-[10px]">
                    <Artwork src={track.artworkUrl} alt={track.title} size={44} radius={10} />
                    <div class="absolute inset-0 flex items-center justify-center bg-black/55 text-white opacity-0 transition-opacity group-hover:opacity-100">
                      <WaveIcon name="play" size={15} filled />
                    </div>
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-[12.5px] font-semibold">{track.title}</div>
                    <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
                  </div>
                  <span class="text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
                    {formatDuration(track.durationSec)}
                  </span>
                </button>
              {/each}
            </div>
          </div>
        </div>
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
              <h2 class="text-[16px] font-bold tracking-[-0.2px]">{shelf.title}</h2>
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
            <div class="grid grid-cols-1 gap-x-2.5 gap-y-1 md:grid-cols-2 xl:grid-cols-3">
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
  {:else if visibleIds.has('quick_picks') && fallbackQuickPicks.length > 0}
    <WaveEntrance index={2}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <div>
            <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">LIBRARY</div>
            <h2 class="text-[15px] font-bold tracking-[-0.2px]">Quick Picks</h2>
          </div>
          <button
            type="button"
            onclick={() => fallbackQuickPicks[0] && playTrackNow(fallbackQuickPicks[0], fallbackQuickPicks)}
            class="cursor-pointer rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            Play all
          </button>
        </div>

        <div class="grid grid-cols-1 gap-x-2 gap-y-1 md:grid-cols-2 xl:grid-cols-3">
          {#each fallbackQuickPicks as track (track.id)}
            <button
              type="button"
              onclick={() => playTrackNow(track, fallbackQuickPicks)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[12px] px-1 py-1 text-left transition-colors hover:bg-white/[0.05]"
            >
              <Artwork src={track.artworkUrl} alt={track.title} size={44} radius={10} />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[12.5px] font-semibold">{track.title}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
              <span class="pr-2 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
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
