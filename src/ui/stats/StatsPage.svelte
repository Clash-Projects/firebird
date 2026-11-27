<script lang="ts">
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import { goAlbum, goArtist, goGenre } from '$src/ui/navigation/waveNav';
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

  interface BackendListeningStats {
    totalPlays?: number;
    totalMinutesListened?: number;
    uniqueArtists?: number;
    uniqueTracks?: number;
    currentStreakDays?: number;
  }

  let period = $state<Period>('1m');
  let tab = $state<StatsTab>('overview');
  let stats = $state<BackendListeningStats | null>(null);

  $effect(() => {
    const p = PERIODS.find((item) => item.id === period)?.backend ?? 'Month';
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void ipcInvoke<BackendListeningStats>('stats_compute_listening', { period: p }).then((res) => {
      if (!cancelled && res) stats = res;
    });
    return () => {
      cancelled = true;
    };
  });

  const rankColors = ['#E0A030', '#C0C4CC', '#CD7F32'];

  let totalPlays = $derived(stats?.totalPlays ?? libraryStore.historyIds.length);
  let totalMinutes = $derived(
    stats?.totalMinutesListened ??
      Math.round(libraryStore.tracksByIds(libraryStore.historyIds).reduce((sum, t) => sum + t.durationSec, 0) / 60)
  );
  let uniqueArtists = $derived(stats?.uniqueArtists ?? libraryStore.artists.length);
</script>

<div class="w-full space-y-5 px-6 pt-[22px] pb-12">
  <header class="flex flex-wrap items-center justify-between gap-3">
    <h1 class="text-[27px] font-bold tracking-[-0.5px] leading-[1.15]">Statistics</h1>

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
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Plays · {period.toUpperCase()}</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{totalPlays}</div>
      </div>
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Listening Time</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{Math.floor(totalMinutes / 60)}h {totalMinutes % 60}m</div>
      </div>
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5">
        <div class="text-[11px] font-semibold text-[var(--lw-text-tertiary)] uppercase">Unique Artists</div>
        <div class="mt-2 text-[28px] font-bold tracking-[-0.6px] lw-tabular">{uniqueArtists}</div>
      </div>
    </div>

    <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-[14px] font-bold">Top Tracks</span>
          <button type="button" onclick={() => (tab = 'tracks')} class="cursor-pointer text-[12px] font-semibold text-[var(--lw-accent)]">See all</button>
        </div>
        <div class="space-y-1.5">
          {#each libraryStore.tracks.slice(0, 5) as track, i (track.id)}
            <div class="flex items-center gap-3 rounded-[10px] px-2 py-1.5 hover:bg-white/[0.04]">
              <span class="w-6 text-center text-[13px] font-bold lw-tabular" style:color={rankColors[i] ?? 'var(--lw-text-tertiary)'}>
                {i + 1}
              </span>
              <Artwork src={track.artworkUrl} size={36} radius={8} />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13px] font-semibold">{track.title}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-[14px] font-bold">Top Artists</span>
          <button type="button" onclick={() => (tab = 'artists')} class="cursor-pointer text-[12px] font-semibold text-[var(--lw-accent)]">See all</button>
        </div>
        <div class="space-y-1.5">
          {#each libraryStore.artists.slice(0, 5) as artist, i (artist.id)}
            <button type="button" onclick={() => goArtist(artist.id)} class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-2 py-1.5 text-left hover:bg-white/[0.04]">
              <span class="w-6 text-center text-[13px] font-bold lw-tabular" style:color={rankColors[i] ?? 'var(--lw-text-tertiary)'}>
                {i + 1}
              </span>
              <Artwork src={artist.artworkUrl} size={36} circle />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13px] font-semibold">{artist.name}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
              </div>
              <WaveIcon name="chevronRight" size={14} class="text-[var(--lw-text-tertiary)]" />
            </button>
          {/each}
        </div>
      </div>
    </div>
  {:else if tab === 'tracks' || tab === 'history'}
    <DesktopTable tracks={libraryStore.tracks} />
  {:else if tab === 'artists'}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
      {#each libraryStore.artists as artist (artist.id)}
        <button type="button" onclick={() => goArtist(artist.id)} class="cursor-pointer rounded-[14px] p-2 text-center hover:bg-white/[0.04]">
          <Artwork src={artist.artworkUrl} circle class="mx-auto aspect-square w-full max-w-[140px]" />
          <div class="mt-2 truncate text-[13px] font-semibold">{artist.name}</div>
        </button>
      {/each}
    </div>
  {:else if tab === 'albums'}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
      {#each libraryStore.albums as album (album.id)}
        <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[14px] p-1.5 text-left hover:bg-white/[0.04]">
          <Artwork src={album.artworkUrl} radius={12} class="aspect-square w-full" />
          <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
          <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{album.artist}</div>
        </button>
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
