<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import type { PodcastEpisode, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let activeShowId = $state(featuresStore.podcasts[0]?.id ?? '');
  let loadingShow = $state(false);
  let loadedEpisodes = $state<Record<string, PodcastEpisode[]>>({});

  let activeShow = $derived(
    featuresStore.podcasts.find((s) => s.id === activeShowId) ?? featuresStore.podcasts[0]
  );
  let episodes = $derived(
    activeShow ? (loadedEpisodes[activeShow.id] ?? activeShow.episodes) : []
  );

  $effect(() => {
    const show = activeShow;
    if (!show || !isTauriRuntime() || loadedEpisodes[show.id]) return;
    let cancelled = false;
    loadingShow = true;
    void ipcInvoke<{
      episodes?: Array<{
        videoId: string;
        title: string;
        description?: string;
        publishedAt?: string;
        durationSeconds?: number;
      }>;
    }>('podcasts_show', { showId: show.id, force: false })
      .then((res) => {
        if (cancelled || !res?.episodes) return;
        loadedEpisodes = {
          ...loadedEpisodes,
          [show.id]: res.episodes.map((ep) => ({
            id: ep.videoId,
            title: ep.title,
            description: ep.description || '',
            publishedAt: ep.publishedAt || '',
            durationSec: Number(ep.durationSeconds) || 0,
            resumeSec: 0,
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

  function playEpisode(ep: PodcastEpisode) {
    if (!activeShow) return;
    const track: Track = {
      id: ep.id,
      videoId: ep.id,
      title: ep.title,
      artist: activeShow.publisher || activeShow.title,
      artists: [activeShow.publisher || activeShow.title],
      album: activeShow.title,
      durationSec: ep.durationSec,
      artworkUrl: activeShow.artworkUrl,
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
  <CommandBar title="Podcasts" subtitle="Resume episodes across devices" />

  {#if featuresStore.podcasts.length === 0}
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
