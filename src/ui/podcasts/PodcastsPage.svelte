<script lang="ts">
  import { onMount } from 'svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import type { PodcastEpisode, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let activeShowId = $state(featuresStore.podcasts[0]?.id ?? '');
  let loadingShow = $state(false);
  let loadedEpisodes = $state<Record<string, PodcastEpisode[]>>({});
  let searchQuery = $state('');
  let searchResults = $state<PodcastEpisode[]>([]);
  let searching = $state(false);

  let activeShow = $derived(
    featuresStore.podcasts.find((s) => s.id === activeShowId) ?? featuresStore.podcasts[0]
  );
  let episodes = $derived(
    activeShow ? (loadedEpisodes[activeShow.id] ?? activeShow.episodes) : []
  );

  onMount(() => {
    if (featuresStore.podcasts.length === 0) {
      void featuresStore.loadPodcasts(false);
    }
  });

  $effect(() => {
    const show = activeShow;
    if (
      !show ||
      !isTauriRuntime() ||
      show.id.startsWith('shelf-') ||
      loadedEpisodes[show.id] ||
      show.episodes.length > 0
    ) {
      return;
    }
    let cancelled = false;
    loadingShow = true;
    void ipcInvoke<{
      episodes?: Array<{
        videoId: string;
        title: string;
        description?: string;
        publishedAt?: string;
        durationSeconds?: number;
        artworkUrl?: string;
      }>;
    }>('podcasts_show', { showId: show.id, force: false })
      .then((res) => {
        if (cancelled || !res?.episodes) return;
        loadedEpisodes = {
          ...loadedEpisodes,
          [show.id]: res.episodes.map((ep) => ({
            id: ep.videoId,
            showId: show.id,
            title: ep.title,
            description: ep.description || '',
            publishedAt: ep.publishedAt || '',
            durationSec: Number(ep.durationSeconds) || 0,
            resumeSec: 0,
            artworkUrl: ep.artworkUrl || show.artworkUrl,
          })),
        };
      })
      .finally(() => {
        if (!cancelled) loadingShow = false;
      });
    return () => {
      cancelled = true;
    };
  });

  async function runSearch(e: SubmitEvent) {
    e.preventDefault();
    const q = searchQuery.trim();
    if (!q || !isTauriRuntime()) {
      searchResults = [];
      return;
    }
    searching = true;
    try {
      const res = await ipcInvoke<
        Array<{
          videoId: string;
          title: string;
          showId?: string;
          showTitle?: string;
          description?: string;
          publishedAt?: string;
          durationSeconds?: number;
          artworkUrl?: string;
        }>
      >('podcasts_search', { query: q });
      searchResults = Array.isArray(res)
        ? res.map((ep) => ({
            id: ep.videoId,
            showId: ep.showId || '',
            title: ep.title,
            description: ep.description || ep.showTitle || '',
            publishedAt: ep.publishedAt || '',
            durationSec: Number(ep.durationSeconds) || 0,
            resumeSec: 0,
            artworkUrl: ep.artworkUrl || '',
          }))
        : [];
    } finally {
      searching = false;
    }
  }

  function playEpisode(ep: PodcastEpisode) {
    const showTitle = activeShow?.title || 'Podcast';
    const track: Track = {
      id: ep.id,
      videoId: ep.id,
      title: ep.title,
      artist: activeShow?.publisher || showTitle,
      artists: [activeShow?.publisher || showTitle],
      album: showTitle,
      durationSec: ep.durationSec,
      artworkUrl: ep.artworkUrl || activeShow?.artworkUrl || '',
      accentColor: '#7c9bff',
      qualityTier: 'high',
      sampleRateHz: 48000,
      bitDepth: 16,
      codec: 'OPUS',
      bitrateKbps: 128,
    };
    playTrackNow(track);
  }
</script>

<div class="space-y-6 px-8 py-8 pb-10">
  <CommandBar title="Podcasts" subtitle="Resume episodes across devices">
    {#snippet actions()}
      <Buttons
        variant="secondary"
        size="sm"
        disabled={featuresStore.podcastsLoading}
        onclick={() => void featuresStore.loadPodcasts(true)}
      >
        {featuresStore.podcastsLoading ? 'Loading...' : 'Refresh'}
      </Buttons>
    {/snippet}
  </CommandBar>

  <form onsubmit={runSearch} class="flex max-w-[460px] gap-2">
    <Input bind:value={searchQuery} placeholder="Search podcast episodes..." />
    <Buttons type="submit" variant="secondary" size="sm" disabled={searching}>
      {searching ? '...' : 'Search'}
    </Buttons>
  </form>

  {#if searchResults.length > 0}
    <div class="space-y-2.5">
      <div class="flex items-center justify-between">
        <h2 class="text-[15px] font-bold">Search Results</h2>
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
      {#each searchResults as ep (ep.id)}
        <div class="flex items-center justify-between gap-4 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div class="flex min-w-0 flex-1 items-center gap-3">
            <Artwork src={ep.artworkUrl} size={48} radius={10} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[14px] font-semibold">{ep.title}</div>
              <p class="mt-0.5 line-clamp-1 text-[12px] text-[var(--lw-text-secondary)]">{ep.description}</p>
            </div>
          </div>
          <Buttons variant="primary" size="sm" onclick={() => playEpisode(ep)}>
            <WaveIcon name="play" size={14} filled /> Play
          </Buttons>
        </div>
      {/each}
    </div>
  {:else if featuresStore.podcastsLoading && featuresStore.podcasts.length === 0}
    <div class="rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-6 py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">
      Loading podcasts...
    </div>
  {:else if featuresStore.podcasts.length === 0}
    <States icon="radio" title="No podcasts yet" description="Subscribed shows and episodes will appear here." />
  {:else}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
      {#each featuresStore.podcasts as show (show.id)}
        <button
          type="button"
          onclick={() => (activeShowId = show.id)}
          class="flex cursor-pointer items-center gap-4 rounded-[16px] border p-4 text-left transition-colors {activeShow?.id === show.id
            ? 'border-[var(--lw-accent)] bg-[var(--lw-wash-pressed)]'
            : 'border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] hover:bg-[var(--lw-wash-hover)]'}"
        >
          <Artwork src={show.artworkUrl} size={72} radius={12} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[15px] font-bold">{show.title}</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">{show.publisher}</div>
            <p class="mt-1 line-clamp-2 text-[12px] text-[var(--lw-text-tertiary)]">{show.description}</p>
          </div>
        </button>
      {/each}
    </div>

    {#if activeShow}
      <div class="space-y-2.5">
        <h2 class="text-[15px] font-bold">Episodes · {activeShow.title}</h2>
        {#if loadingShow && episodes.length === 0}
          <div class="py-6 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading episodes...</div>
        {:else}
          {#each episodes as ep (ep.id)}
            <div class="flex items-center justify-between gap-4 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
              <div class="min-w-0 flex-1">
                <div class="text-[14px] font-semibold">{ep.title}</div>
                <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">{ep.description}</p>
                <div class="mt-2 flex items-center gap-3 text-[11.5px] text-[var(--lw-text-tertiary)] lw-tabular">
                  <span>{ep.publishedAt}</span>
                  <span>{formatDuration(ep.durationSec)}</span>
                  {#if ep.resumeSec > 0}
                    <span class="text-[var(--lw-accent)]">Resume at {formatDuration(ep.resumeSec)}</span>
                  {/if}
                </div>
              </div>
              <Buttons variant="primary" size="sm" onclick={() => playEpisode(ep)}>
                <WaveIcon name="play" size={14} filled /> Play
              </Buttons>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  {/if}
</div>
