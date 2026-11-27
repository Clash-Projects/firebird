<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goAlbum, goArtist, goGenre, goPlaylist } from '$src/ui/navigation/waveNav';
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
  let featured = $derived(libraryStore.tracks[0]);
  let companions = $derived(libraryStore.tracks.slice(1, 4));
  let quickPicks = $derived(libraryStore.tracks.slice(0, 6));
  let charts = $derived(libraryStore.tracks.slice(0, 5));
  let visibleIds = $derived(new Set(settingsStore.homeShelves.filter((s) => s.visible).map((s) => s.id)));
</script>

<div class="w-full space-y-7 px-4 pt-[22px] pb-12 md:px-6 xl:px-7">
  <WaveEntrance index={0}>
    <header class="flex items-center justify-between gap-4">
      <div class="min-w-0">
        <h1 class="truncate text-[24px] font-bold tracking-[-0.4px] leading-[1.15]">
          {username ? `${greeting}, ${username}` : greeting}
        </h1>
        <p class="mt-0.5 text-[12px] text-[var(--lw-text-secondary)]">
          {username ? `For ${username} · ${dateLabel} · picked from your taste` : `${dateLabel} · picked for you`}
        </p>
      </div>
      <button
        type="button"
        onclick={() => router.navigate('/home/layout')}
        class="inline-flex h-8 shrink-0 cursor-pointer items-center gap-1.5 rounded-[10px] px-3 text-[12px] font-semibold text-[var(--lw-text-secondary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name="layers" size={14} />
        Edit home
      </button>
    </header>
  </WaveEntrance>

  {#if !featured && libraryStore.albums.length === 0 && libraryStore.artists.length === 0}
    <States
      icon="music"
      title="Your library is empty"
      description="Sign in to YouTube Music or add local music folders in Settings to start listening."
    />
  {/if}

  {#if featured}
    <WaveEntrance index={1}>
      <section class="relative overflow-hidden rounded-[20px] bg-white/[0.035] p-5">
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
                FOR YOU · FEATURED MIX
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
                  onclick={() => playTrackNow(featured, libraryStore.tracks)}
                  class="inline-flex cursor-pointer items-center gap-2 rounded-full bg-[var(--lw-accent)] px-[22px] py-[10px] text-[12px] font-semibold tracking-[0.2px] text-[var(--lw-on-accent)] transition-all hover:brightness-95 active:scale-[0.98]"
                >
                  <WaveIcon name="play" size={14} filled />
                  Play
                </button>
                <button
                  type="button"
                  onclick={() => {
                    playTrackNow(featured, [...libraryStore.tracks].reverse());
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
                  onclick={() => playTrackNow(track, libraryStore.tracks)}
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

  {#if visibleIds.has('quick_picks') && quickPicks.length > 0}
    <WaveEntrance index={2}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <div>
            <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">START RADIO FROM A SONG</div>
            <h2 class="text-[15px] font-bold tracking-[-0.2px]">Quick Picks</h2>
          </div>
          <button
            type="button"
            onclick={() => quickPicks[0] && playTrackNow(quickPicks[0], quickPicks)}
            class="cursor-pointer rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            Play all
          </button>
        </div>

        <div class="grid grid-cols-1 gap-x-2 gap-y-1 md:grid-cols-2 xl:grid-cols-3">
          {#each quickPicks as track (track.id)}
            {@const playing = playerStore.currentTrack?.id === track.id}
            <button
              type="button"
              onclick={() => playTrackNow(track, quickPicks)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[56px] w-full cursor-pointer items-center gap-2.5 rounded-[12px] px-1 py-1 text-left transition-colors {playing
                ? 'bg-[var(--lw-accent)]/12'
                : 'hover:bg-white/[0.05]'}"
            >
              <div class="relative h-[48px] w-[48px] shrink-0 overflow-hidden rounded-[12px]">
                <Artwork src={track.artworkUrl} alt={track.title} size={48} radius={12} />
                <div
                  class="absolute inset-0 flex items-center justify-center bg-black/55 text-white transition-opacity {playing
                    ? 'opacity-100'
                    : 'opacity-0 group-hover:opacity-100'}"
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
              <span class="pr-2 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
                {formatDuration(track.durationSec)}
              </span>
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if visibleIds.has('albums') && libraryStore.albums.length > 0}
    <WaveEntrance index={3}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <div>
            <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">ROTATION</div>
            <h2 class="text-[15px] font-bold tracking-[-0.2px]">Albums For You</h2>
          </div>
          <button
            type="button"
            onclick={() => router.navigate('/albums')}
            class="cursor-pointer rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            See all
          </button>
        </div>

        <div class="lw-scroll-x flex gap-3 py-1.5">
          {#each libraryStore.albums as album (album.id)}
            <button
              type="button"
              onclick={() => goAlbum(album.id)}
              class="group w-[160px] shrink-0 cursor-pointer text-left"
            >
              <div class="relative overflow-hidden rounded-[12px] transition-all duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-hover:shadow-[var(--lw-shadow-lift)] group-active:scale-[0.97]">
                <Artwork src={album.artworkUrl} alt={album.title} size={160} radius={12} />
                <div class="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-transparent to-black/55 opacity-0 transition-opacity duration-[110ms] group-hover:opacity-100">
                  <div class="flex h-11 w-11 scale-75 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[0_4px_12px_rgba(0,0,0,0.4)] transition-transform duration-[110ms] ease-[cubic-bezier(0.34,1.56,0.64,1)] group-hover:scale-100">
                    <WaveIcon name="play" size={18} filled />
                  </div>
                </div>
              </div>
              <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist} · {album.year}</div>
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if visibleIds.has('personal_mix') && libraryStore.artists.length > 0}
    <WaveEntrance index={4}>
      <section class="space-y-2.5">
        <div class="flex items-end justify-between">
          <div>
            <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">ARTISTS</div>
            <h2 class="text-[15px] font-bold tracking-[-0.2px]">Artists For You</h2>
          </div>
          <button
            type="button"
            onclick={() => router.navigate('/artists')}
            class="cursor-pointer rounded-[8px] px-2 py-1 text-[12px] font-semibold text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            See all
          </button>
        </div>

        <div class="lw-scroll-x flex gap-4 py-1.5">
          {#each libraryStore.artists as artist (artist.id)}
            <button
              type="button"
              onclick={() => goArtist(artist.id)}
              class="group w-[144px] shrink-0 cursor-pointer text-center"
            >
              <div class="mx-auto w-[124px] transition-transform duration-[110ms] ease-[var(--ease-wave)] group-hover:scale-[1.03] group-active:scale-[0.97]">
                <Artwork src={artist.artworkUrl} alt={artist.name} size={124} circle shadow="lift" />
              </div>
              <div class="mt-2.5 truncate text-[13px] font-semibold">{artist.name}</div>
              <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{artist.monthlyListeners}</div>
            </button>
          {/each}
        </div>
      </section>
    </WaveEntrance>
  {/if}

  {#if visibleIds.has('recent_plays') && charts.length > 0}
    <WaveEntrance index={5}>
      <section class="space-y-2">
        <div>
          <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">CHARTS</div>
          <h2 class="text-[15px] font-bold tracking-[-0.2px]">Trending Now</h2>
        </div>

        <div class="space-y-0.5">
          {#each charts as track, i (track.id)}
            <button
              type="button"
              onclick={() => playTrackNow(track, charts)}
              oncontextmenu={(e) => contextMenu.showForTrack(e, track)}
              class="group flex h-[54px] w-full cursor-pointer items-center gap-3.5 rounded-[12px] px-2 text-left transition-colors hover:bg-white/[0.05]"
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
