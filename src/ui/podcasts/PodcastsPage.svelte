<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let activeShowId = $state(featuresStore.podcasts[0]?.id ?? '');
  let activeShow = $derived(featuresStore.podcasts.find((s) => s.id === activeShowId) ?? featuresStore.podcasts[0]);
</script>

<div class="mx-auto max-w-[1280px] space-y-6 px-6 py-4 pb-10">
  <CommandBar title="Podcasts" subtitle="Resume episodes across devices" />

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
          <Buttons
            variant="primary"
            size="sm"
            onclick={() =>
              featuresStore.showToast({
                title: 'Playing episode',
                subtitle: ep.title,
                severity: 'info',
              })}
          >
            <WaveIcon name="play" size={14} filled /> Play
          </Buttons>
        </div>
      {/each}
    </div>
  {/if}
</div>
