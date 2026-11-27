<script lang="ts">
  import { onMount } from 'svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import type { PodcastEpisode, PodcastShow, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface BackendEpisode {
    videoId: string;
    title: string;
    showId?: string;
    showTitle?: string;
    description?: string;
    publishedLabel?: string;
    durationMs?: number;
    artworkUrl?: string;
  }

  let selectedShow = $state<PodcastShow | null>(null);
  let loadingShow = $state(false);
  let loadingMore = $state(false);
  let showEpisodes = $state<Record<string, PodcastEpisode[]>>({});
  let showContinuation = $state<Record<string, string | null>>({});

  let searchQuery = $state('');
  let searchResults = $state<PodcastEpisode[]>([]);
  let searching = $state(false);

  let activeEpisodes = $derived(
    selectedShow ? (showEpisodes[selectedShow.id] ?? selectedShow.episodes) : []
  );
  let activeContinuation = $derived(
    selectedShow ? (showContinuation[selectedShow.id] ?? null) : null
  );

  onMount(() => {
    if (featuresStore.podcastShelves.length === 0) {
      void featuresStore.loadPodcasts(false);
    }
  });

  function mapEpisode(ep: BackendEpisode, fallbackShow?: PodcastShow | null): PodcastEpisode {
    return {
      id: ep.videoId,
      showId: ep.showId || fallbackShow?.id || '',
      showTitle: ep.showTitle || fallbackShow?.title || '',
      title: ep.title,
      description: ep.description || ep.showTitle || '',
      publishedAt: ep.publishedLabel || '',
      durationSec: Math.max(0, Math.round(Number(ep.durationMs || 0) / 1000)),
      resumeSec: 0,
      artworkUrl: ep.artworkUrl || fallbackShow?.artworkUrl || '',
    };
  }

  async function openShow(show: PodcastShow) {
    selectedShow = show;
    if (!isTauriRuntime() || showEpisodes[show.id]) return;
    loadingShow = true;
    try {
      const res = await ipcInvoke<{
        episodes?: BackendEpisode[];
        continuation?: string | null;
      }>('podcasts_show', { showId: show.id, force: false });
      if (res?.episodes) {
        showEpisodes = {
          ...showEpisodes,
          [show.id]: res.episodes.map((ep) => mapEpisode(ep, show)),
        };
        showContinuation = {
          ...showContinuation,
          [show.id]: res.continuation ?? null,
        };
      }
    } finally {
      loadingShow = false;
    }
  }

  async function loadMoreEpisodes() {
    if (!selectedShow || !activeContinuation || loadingMore || !isTauriRuntime()) return;
    loadingMore = true;
    try {
      const res = await ipcInvoke<{
        episodes?: BackendEpisode[];
        continuation?: string | null;
      }>('podcasts_more', {
        showId: selectedShow.id,
        continuation: activeContinuation,
      });
      if (res?.episodes) {
        const existing = showEpisodes[selectedShow.id] ?? [];
        const seen = new Set(existing.map((e) => e.id));
        const added = res.episodes
          .map((ep) => mapEpisode(ep, selectedShow))
          .filter((ep) => !seen.has(ep.id));
        showEpisodes = {
          ...showEpisodes,
          [selectedShow.id]: [...existing, ...added],
        };
        showContinuation = {
          ...showContinuation,
          [selectedShow.id]: res.continuation ?? null,
        };
      }
    } finally {
      loadingMore = false;
    }
  }

  async function runSearch(e: SubmitEvent) {
    e.preventDefault();
    const q = searchQuery.trim();
    if (!q || !isTauriRuntime()) {
      searchResults = [];
      return;
    }
    searching = true;
    try {
      const res = await ipcInvoke<BackendEpisode[]>('podcasts_search', { query: q });
      searchResults = Array.isArray(res) ? res.map((ep) => mapEpisode(ep)) : [];
    } finally {
      searching = false;
    }
  }

  function toTrack(ep: PodcastEpisode): Track {
    const showName = ep.showTitle || selectedShow?.title || 'Podcast';
    const artist = selectedShow?.publisher || showName;
    return {
      id: ep.id,
      videoId: ep.id,
      title: ep.title,
      artist,
      artists: [artist],
      album: showName,
      durationSec: ep.durationSec,
      artworkUrl: ep.artworkUrl || selectedShow?.artworkUrl || '',
      accentColor: '#7c9bff',
      qualityTier: 'high',
      sampleRateHz: 48000,
      bitDepth: 16,
      codec: 'OPUS',
      bitrateKbps: 128,
    };
  }

  function playEpisode(ep: PodcastEpisode, list?: PodcastEpisode[]) {
    if (isTauriRuntime()) {
      void ipcInvoke('podcasts_remember', {
        episode: {
          videoId: ep.id,
          title: ep.title,
          showId: ep.showId,
          showTitle: ep.showTitle || selectedShow?.title || '',
          description: ep.description,
          artworkUrl: ep.artworkUrl,
          durationMs: Math.round(ep.durationSec * 1000),
          publishedLabel: ep.publishedAt,
        },
      });
    }
    const queue = (list && list.length > 0 ? list : [ep]).map(toTrack);
    playTrackNow(toTrack(ep), queue);
  }
</script>

<div class="space-y-6 px-6 py-6 pb-12 md:px-8">
  <CommandBar title="Podcasts" subtitle="YouTube Music shows & episodes">
    {#snippet actions()}
      <Buttons
        variant="secondary"
        size="sm"
        disabled={featuresStore.podcastsLoading}
        onclick={() => {
          selectedShow = null;
          void featuresStore.loadPodcasts(true);
        }}
      >
        {featuresStore.podcastsLoading ? 'Loading...' : 'Refresh'}
      </Buttons>
    {/snippet}
  </CommandBar>

  <form onsubmit={runSearch} class="flex max-w-[460px] gap-2">
    <Input bind:value={searchQuery} placeholder="Search podcasts and episodes..." />
    <Buttons type="submit" variant="secondary" size="sm" disabled={searching}>
      {searching ? '...' : 'Search'}
    </Buttons>
  </form>

  {#if searchResults.length > 0}
    <div class="space-y-2.5">
      <div class="flex items-center justify-between">
        <h2 class="text-[16px] font-bold">Search Results</h2>
        <button
          type="button"
          onclick={() => {
            searchResults = [];
            searchQuery = '';
          }}
          class="cursor-pointer text-[12px] text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
        >
          Clear
        </button>
      </div>
      <div class="space-y-2">
        {#each searchResults as ep (ep.id)}
          <div class="flex items-center justify-between gap-4 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3.5">
            <div class="flex min-w-0 flex-1 items-center gap-3">
              <Artwork src={ep.artworkUrl} size={52} radius={10} />
              <div class="min-w-0 flex-1">
                <div class="truncate text-[14px] font-semibold">{ep.title}</div>
                <p class="mt-0.5 line-clamp-1 text-[12px] text-[var(--lw-text-secondary)]">
                  {ep.showTitle || ep.description}
                </p>
                {#if ep.publishedAt || ep.durationSec > 0}
                  <div class="mt-1 flex gap-3 text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">
                    {#if ep.publishedAt}<span>{ep.publishedAt}</span>{/if}
                    {#if ep.durationSec > 0}<span>{formatDuration(ep.durationSec)}</span>{/if}
                  </div>
                {/if}
              </div>
            </div>
            <Buttons variant="primary" size="sm" onclick={() => playEpisode(ep, searchResults)}>
              <WaveIcon name="play" size={14} filled /> Play
            </Buttons>
          </div>
        {/each}
      </div>
    </div>
  {:else if selectedShow}
    <div class="space-y-4">
      <button
        type="button"
        onclick={() => (selectedShow = null)}
        class="inline-flex cursor-pointer items-center gap-1.5 text-[12.5px] font-semibold text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name="chevronLeft" size={15} /> Back to Podcasts
      </button>

      <div class="flex items-center gap-4 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
        <Artwork src={selectedShow.artworkUrl} size={84} radius={12} />
        <div class="min-w-0 flex-1">
          <div class="text-[19px] font-bold">{selectedShow.title}</div>
          {#if selectedShow.publisher}
            <div class="text-[12.5px] text-[var(--lw-text-secondary)]">{selectedShow.publisher}</div>
          {/if}
          {#if selectedShow.description}
            <p class="mt-1 line-clamp-2 text-[12px] text-[var(--lw-text-tertiary)]">{selectedShow.description}</p>
          {/if}
        </div>
      </div>

      {#if loadingShow && activeEpisodes.length === 0}
        <div class="py-8 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading episodes...</div>
      {:else}
        <div class="space-y-2">
          {#each activeEpisodes as ep (ep.id)}
            <div class="flex items-center justify-between gap-4 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
              <div class="min-w-0 flex-1">
                <div class="text-[14px] font-semibold">{ep.title}</div>
                {#if ep.description}
                  <p class="mt-1 line-clamp-2 text-[12.5px] text-[var(--lw-text-secondary)]">{ep.description}</p>
                {/if}
                <div class="mt-1.5 flex items-center gap-3 text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">
                  {#if ep.publishedAt}<span>{ep.publishedAt}</span>{/if}
                  {#if ep.durationSec > 0}<span>{formatDuration(ep.durationSec)}</span>{/if}
                </div>
              </div>
              <Buttons variant="primary" size="sm" onclick={() => playEpisode(ep, activeEpisodes)}>
                <WaveIcon name="play" size={14} filled /> Play
              </Buttons>
            </div>
          {/each}
        </div>
        {#if activeContinuation}
          <div class="flex justify-center pt-2">
            <Buttons variant="secondary" size="sm" disabled={loadingMore} onclick={loadMoreEpisodes}>
              {loadingMore ? 'Loading more...' : 'Load more episodes'}
            </Buttons>
          </div>
        {/if}
      {/if}
    </div>
  {:else if featuresStore.podcastsLoading && featuresStore.podcastShelves.length === 0}
    <div class="rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-6 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      Loading YouTube Music podcasts...
    </div>
  {:else if featuresStore.podcastShelves.length === 0}
    <div class="space-y-3 text-center">
      <States
        icon="radio"
        title="No podcasts available"
        description={featuresStore.podcastsError ?? 'YouTube Music podcast shelves did not return any shows or episodes.'}
      />
      <Buttons variant="secondary" size="sm" onclick={() => void featuresStore.loadPodcasts(true)}>
        Retry
      </Buttons>
    </div>
  {:else}
    <div class="space-y-7">
      {#each featuresStore.podcastShelves as shelf, i (`${shelf.title}-${i}`)}
        <section class="space-y-2.5">
          <h2 class="text-[16px] font-bold tracking-[-0.2px]">{shelf.title}</h2>

          {#if shelf.shows.length > 0}
            <div use:waveSmoothScroll={{ axis: 'x', drag: true }} class="lw-scroll-x flex gap-3.5 py-1">
              {#each shelf.shows as show (show.id)}
                <button
                  type="button"
                  onclick={() => void openShow(show)}
                  class="group w-[176px] shrink-0 cursor-pointer text-left"
                >
                  <div class="overflow-hidden rounded-[12px] transition-transform duration-[140ms] group-hover:scale-[1.03]">
                    <Artwork src={show.artworkUrl} alt={show.title} size={176} radius={12} />
                  </div>
                  <div class="mt-2 truncate text-[13px] font-semibold">{show.title}</div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">
                    {show.publisher || 'Podcast'}
                  </div>
                </button>
              {/each}
            </div>
          {/if}

          {#if shelf.episodes.length > 0}
            <div use:waveSmoothScroll={{ axis: 'x', drag: true }} class="lw-scroll-x flex gap-3.5 py-1">
              {#each shelf.episodes as ep (`${shelf.title}-${ep.id}`)}
                <button
                  type="button"
                  onclick={() => playEpisode(ep, shelf.episodes)}
                  class="group w-[220px] shrink-0 cursor-pointer text-left"
                >
                  <div class="relative aspect-video w-full overflow-hidden rounded-[12px] bg-[var(--lw-surface)]">
                    <Artwork src={ep.artworkUrl} alt={ep.title} radius={12} class="h-full w-full" />
                    <div class="absolute inset-0 flex items-center justify-center bg-black/45 opacity-0 transition-opacity group-hover:opacity-100">
                      <div class="flex h-10 w-10 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)]">
                        <WaveIcon name="play" size={16} filled />
                      </div>
                    </div>
                  </div>
                  <div class="mt-2 line-clamp-1 text-[13px] font-semibold">{ep.title}</div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">
                    {ep.showTitle || ep.publishedAt || (ep.durationSec > 0 ? formatDuration(ep.durationSec) : 'Episode')}
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>
