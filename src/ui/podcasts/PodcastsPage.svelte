<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import States from '$src/ui/components/States.svelte';

  let activeShowId = $state(featuresStore.podcasts[0]?.id ?? '');
  let activeShow = $derived(featuresStore.podcasts.find((s) => s.id === activeShowId) ?? featuresStore.podcasts[0]);
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
        {#each activeShow.episodes as ep (ep.id)}
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
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>
