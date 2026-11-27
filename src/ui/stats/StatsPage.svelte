<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import type { Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import { goGenre } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  type Period = '7d' | '1m' | 'all';
  type StatsTab = 'overview' | 'tracks' | 'artists' | 'albums' | 'history' | 'genres';

  const PERIODS: ReadonlyArray<{ id: Period; label: string; backend: string }> = [
    { id: '7d', label: '7 Days', backend: 'Week' },
    { id: '1m', label: '30 Days', backend: 'Month' },
    { id: 'all', label: 'All Time', backend: 'AllTime' },
  ];

  const TABS: ReadonlyArray<{ id: StatsTab; label: string }> = [
    { id: 'overview', label: 'Overview' },
    { id: 'tracks', label: 'Top Tracks' },
    { id: 'artists', label: 'Top Artists' },
    { id: 'albums', label: 'Top Albums' },
    { id: 'history', label: 'History' },
    { id: 'genres', label: 'Genres' },
  ];

  interface StatTrack {
    name: string;
    artist: string;
    artworkUrl: string;
    playCount: number;
  }

  interface StatArtist {
    name: string;
    playCount: number;
    artworkUrl: string;
  }

  interface StatAlbum {
    name: string;
    artist: string;
    artworkUrl: string;
    playCount: number;
  }

  interface RecentStatPlay {
    title: string;
    artist: string;
    timestampMillis: number;
    artworkUrl: string;
  }

  interface BackendListeningStats {
    totalPlays?: number;
    totalMinutesListened?: number;
    prevPeriodPlays?: number | null;
    uniqueArtists?: number;
    uniqueTracks?: number;
    currentStreakDays?: number;
    peakListeningLabel?: string;
    topTracks?: StatTrack[];
    topArtists?: StatArtist[];
    topAlbums?: StatAlbum[];
    recentPlays?: RecentStatPlay[];
  }

  let period = $state<Period>('1m');
  let tab = $state<StatsTab>('overview');
  let stats = $state<BackendListeningStats | null>(null);
  let loading = $state(false);
  let refreshNonce = $state(0);

  $effect(() => {
    void refreshNonce;
    void playerStore.currentTrack?.id;
    const p = PERIODS.find((item) => item.id === period)?.backend ?? 'Month';
    if (!isTauriRuntime()) return;
    let cancelled = false;
    loading = true;
    void ipcInvoke<BackendListeningStats>('stats_compute_listening', { period: p })
      .then((res) => {
        if (!cancelled && res) stats = res;
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  const rankColors = ['#E0A030', '#C0C4CC', '#CD7F32'];

  let totalPlays = $derived(stats?.totalPlays ?? 0);
  let totalMinutes = $derived(stats?.totalMinutesListened ?? 0);
  let uniqueArtists = $derived(stats?.uniqueArtists ?? 0);
  let uniqueTracks = $derived(stats?.uniqueTracks ?? 0);
  let streakDays = $derived(stats?.currentStreakDays ?? 0);
  let peakLabel = $derived(stats?.peakListeningLabel || '—');
  let topTracks = $derived(stats?.topTracks ?? []);
  let topArtists = $derived(stats?.topArtists ?? []);
  let topAlbums = $derived(stats?.topAlbums ?? []);
  let recentPlays = $derived(stats?.recentPlays ?? []);

  function asPlayableTrack(title: string, artist: string, artworkUrl: string, album = ''): Track {
    const existing = libraryStore.tracks.find(
      (t) =>
        t.title.toLowerCase() === title.toLowerCase() &&
        t.artist.toLowerCase() === artist.toLowerCase()
    );
    if (existing) return existing;
    return {
      id: `stat:${title}|${artist}`,
      videoId: '',
      title,
      artist,
      artists: artist.split(/,\s*/).filter(Boolean),
      album,
      durationSec: 0,
      artworkUrl,
      accentColor: '#7c9bff',
      qualityTier: 'high',
      sampleRateHz: 48000,
      bitDepth: 16,
      codec: 'OPUS',
      bitrateKbps: 256,
    };
  }

  function formatRelative(ms: number): string {
    if (!ms) return '';
    const diffSec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
    if (diffSec < 60) return 'Just now';
    if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
    if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
    const days = Math.floor(diffSec / 86400);
    if (days < 7) return `${days}d ago`;
    return new Date(ms).toLocaleDateString();
  }
</script>

<div class="w-full space-y-5 px-6 pt-[22px] pb-12">
  <header class="flex flex-wrap items-center justify-between gap-3">
    <h1 class="text-[27px] font-bold tracking-[-0.5px] leading-[1.15]">Statistics</h1>

    <div class="flex flex-wrap items-center gap-2">
      <div class="flex flex-wrap items-center gap-1 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-1">
        {#each PERIODS as p (p.id)}
          <button
            type="button"
            onclick={() => (period = p.id)}
            class="cursor-pointer rounded-full px-3 py-1 text-[12px] font-semibold transition-colors {period === p.id
              ? 'bg-[var(--lw-accent)] text-[var(--lw-on-accent)]'
              : 'text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
          >
            {p.label}
          </button>
        {/each}
      </div>

      <Buttons variant="ghost" size="sm" disabled={loading} onclick={() => (refreshNonce += 1)}>
        <WaveIcon name="refresh" size={14} class={loading ? 'animate-spin' : ''} />
      </Buttons>
    </div>
  </header>

  <div class="flex items-center border-b border-[var(--lw-divider)]">
    {#each TABS as item (item.id)}
      {@const active = tab === item.id}
      <button
        type="button"
        onclick={() => (tab = item.id)}
        class="mr-5 cursor-pointer border-b-2 px-0.5 py-2 text-[13.5px] transition-colors {active
          ? 'border-[var(--lw-accent)] font-semibold text-[var(--lw-text-primary)]'
          : 'border-transparent text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
      >
        {item.label}
      </button>
    {/each}
  </div>

  {#if tab === 'overview'}
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Plays · {period.toUpperCase()}</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{totalPlays}</div>
        <div class="mt-0.5 text-[11.5px] text-[var(--lw-text-tertiary)]">{uniqueTracks} unique tracks</div>
      </div>
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Listening Time</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{Math.floor(totalMinutes / 60)}h {totalMinutes % 60}m</div>
        <div class="mt-0.5 text-[11.5px] text-[var(--lw-text-tertiary)]">{uniqueArtists} unique artists</div>
      </div>
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Listening Streak</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{streakDays}d</div>
        <div class="mt-0.5 text-[11.5px] text-[var(--lw-text-tertiary)]">Active local streak</div>
      </div>
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Peak Hour</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{peakLabel}</div>
        <div class="mt-0.5 text-[11.5px] text-[var(--lw-text-tertiary)]">Most active window</div>
      </div>
    </div>

    <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-[14px] font-bold">Top Tracks</span>
          <button type="button" onclick={() => (tab = 'tracks')} class="cursor-pointer text-[12px] font-semibold text-[var(--lw-accent)]">See all</button>
        </div>
        {#if topTracks.length === 0}
          <div class="py-8 text-center text-[12.5px] text-[var(--lw-text-tertiary)]">Play songs to build local track statistics.</div>
        {:else}
          <div class="space-y-1.5">
            {#each topTracks.slice(0, 5) as track, i (`${track.name}-${track.artist}-${i}`)}
              <button
                type="button"
                onclick={() => playTrackNow(asPlayableTrack(track.name, track.artist, track.artworkUrl))}
                class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-2 py-1.5 text-left hover:bg-white/[0.04]"
              >
                <span class="w-6 text-center text-[13px] font-bold lw-tabular" style:color={rankColors[i] ?? 'var(--lw-text-tertiary)'}>
                  {i + 1}
                </span>
                <Artwork src={track.artworkUrl} size={36} radius={8} />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-semibold">{track.name}</div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
                </div>
                <span class="text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">{track.playCount} plays</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-[14px] font-bold">Top Artists</span>
          <button type="button" onclick={() => (tab = 'artists')} class="cursor-pointer text-[12px] font-semibold text-[var(--lw-accent)]">See all</button>
        </div>
        {#if topArtists.length === 0}
          <div class="py-8 text-center text-[12.5px] text-[var(--lw-text-tertiary)]">No artist play counts recorded yet.</div>
        {:else}
          <div class="space-y-1.5">
            {#each topArtists.slice(0, 5) as artist, i (`${artist.name}-${i}`)}
              <div class="flex items-center gap-3 rounded-[10px] px-2 py-1.5 hover:bg-white/[0.04]">
                <span class="w-6 text-center text-[13px] font-bold lw-tabular" style:color={rankColors[i] ?? 'var(--lw-text-tertiary)'}>
                  {i + 1}
                </span>
                <Artwork src={artist.artworkUrl} size={36} circle />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-semibold">{artist.name}</div>
                </div>
                <span class="text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">{artist.playCount} plays</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {:else if tab === 'tracks'}
    <div class="space-y-1.5">
      {#each topTracks as track, i (`t-${track.name}-${track.artist}-${i}`)}
        <button
          type="button"
          onclick={() => playTrackNow(asPlayableTrack(track.name, track.artist, track.artworkUrl))}
          class="flex w-full cursor-pointer items-center gap-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="w-7 text-center text-[13px] font-bold lw-tabular" style:color={rankColors[i] ?? 'var(--lw-text-tertiary)'}>{i + 1}</span>
          <Artwork src={track.artworkUrl} size={42} radius={8} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13.5px] font-semibold">{track.name}</div>
            <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{track.artist}</div>
          </div>
          <span class="text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{track.playCount} plays</span>
        </button>
      {/each}
    </div>
  {:else if tab === 'history'}
    <div class="space-y-1.5">
      {#each recentPlays as play, i (`h-${play.title}-${play.timestampMillis}-${i}`)}
        <button
          type="button"
          onclick={() => playTrackNow(asPlayableTrack(play.title, play.artist, play.artworkUrl))}
          class="flex w-full cursor-pointer items-center gap-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <Artwork src={play.artworkUrl} size={40} radius={8} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13.5px] font-semibold">{play.title}</div>
            <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{play.artist}</div>
          </div>
          <span class="text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">{formatRelative(play.timestampMillis)}</span>
        </button>
      {/each}
    </div>
  {:else if tab === 'artists'}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
      {#each topArtists as artist, i (`a-${artist.name}-${i}`)}
        <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3 text-center">
          <Artwork src={artist.artworkUrl} circle class="mx-auto aspect-square w-full max-w-[130px]" />
          <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
          <div class="text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">{artist.playCount} plays</div>
        </div>
      {/each}
    </div>
  {:else if tab === 'albums'}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
      {#each topAlbums as album, i (`al-${album.name}-${album.artist}-${i}`)}
        <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-2.5 text-left">
          <Artwork src={album.artworkUrl} radius={10} class="aspect-square w-full" />
          <div class="mt-2 truncate text-[13px] font-semibold">{album.name}</div>
          <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
          <div class="mt-0.5 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">{album.playCount} plays</div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
      {#each libraryStore.genres as g (g.id)}
        <button
          type="button"
          onclick={() => goGenre(g.id)}
          class="h-24 cursor-pointer rounded-[14px] p-3.5 text-left"
          style:background="linear-gradient(135deg, {g.color}, #121212)"
        >
          <div class="text-[14px] font-bold text-white">{g.name}</div>
          <div class="text-[11.5px] text-white/75">{g.trackIds.length} tracks</div>
        </button>
      {/each}
    </div>
  {/if}
</div>
