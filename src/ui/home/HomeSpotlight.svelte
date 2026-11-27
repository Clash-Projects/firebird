<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { featuresStore } from '$src/state/features.svelte';
  import type { Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let {
    track,
    queue,
    kicker = 'FOR YOU · YOUTUBE MUSIC MIX',
  }: { track?: Track; queue: Track[]; kicker?: string } = $props();

  let companions = $derived(
    queue.filter((item) => !track || item.id !== track.id).slice(0, 3)
  );

  function playHero(radio = false) {
    if (!track) return;
    playTrackNow(track, radio ? [...queue].reverse() : queue);
    if (radio) {
      featuresStore.showToast({ title: 'Started radio', subtitle: track.title, severity: 'info' });
    }
  }
</script>

<section
  class="relative overflow-hidden rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-5"
  aria-labelledby="spotlight-title"
>
  {#if track}
    <div class="flex flex-col gap-5 lg:flex-row lg:items-center">
      <div class="flex min-w-0 flex-[58] items-center gap-5">
        <div class="relative h-[148px] w-[148px] shrink-0 sm:h-[168px] sm:w-[168px]">
          <div
            class="pointer-events-none absolute -inset-16 rounded-full opacity-30 blur-3xl"
            style:background="radial-gradient(circle, var(--lw-accent) 0%, transparent 70%)"
          ></div>
          <div class="relative h-full w-full overflow-hidden rounded-[12px] shadow-[var(--lw-shadow-artwork)]">
            <Artwork
              src={track.artworkUrl}
              label={track.title}
              artist={track.artist}
              kind="track"
              alt={`Cover art for ${track.title}`}
              radius={12}
              class="h-full w-full"
            />
          </div>
        </div>

        <div class="min-w-0 flex-1 space-y-1.5">
          <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
            {kicker}
          </div>
          <h2
            id="spotlight-title"
            class="line-clamp-2 text-[22px] leading-[1.18] font-bold tracking-[-0.02em] sm:text-[24px]"
          >
            {track.title}
          </h2>
          <p class="truncate text-[13.5px] text-[var(--lw-text-secondary)]">
            {track.artist}
          </p>
          <div class="flex flex-wrap items-center gap-2.5 pt-2.5">
            <button
              type="button"
              onclick={() => playHero(false)}
              class="lw-press inline-flex h-9 cursor-pointer items-center gap-2 rounded-full bg-[var(--lw-accent)] px-5 text-[13px] font-bold text-[var(--lw-on-accent)] transition-all hover:brightness-105"
            >
              <WaveIcon name="play" size={14} filled />
              Play
            </button>
            <button
              type="button"
              onclick={() => playHero(true)}
              class="lw-press inline-flex h-9 cursor-pointer items-center gap-2 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface-overlay)] px-4 text-[13px] font-semibold text-[var(--lw-text-primary)] transition-colors hover:bg-[var(--lw-wash-hover)]"
            >
              <WaveIcon name="radio" size={14} />
              Radio
            </button>
          </div>
        </div>
      </div>

      {#if companions.length > 0}
        <div class="hidden h-[168px] w-px bg-[var(--lw-divider)] lg:block"></div>
        <div class="hidden min-w-0 flex-[42] flex-col lg:flex">
          <div class="mb-2 text-[9.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
            MORE FOR YOU
          </div>
          <div class="space-y-0.5">
            {#each companions as item (item.id)}
              <button
                type="button"
                onclick={() => playTrackNow(item, queue)}
                class="group flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-2 py-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
              >
                <div class="relative h-[42px] w-[42px] shrink-0 overflow-hidden rounded-[8px]">
                  <Artwork src={item.artworkUrl} label={item.title} artist={item.artist} kind="track" alt={item.title} size={42} radius={8} />
                  <div class="absolute inset-0 flex items-center justify-center bg-black/55 text-white opacity-0 transition-opacity group-hover:opacity-100">
                    <WaveIcon name="play" size={14} filled />
                  </div>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-semibold text-[var(--lw-text-primary)]">
                    {item.title}
                  </div>
                  <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">
                    {item.artist}
                  </div>
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="flex flex-col justify-between gap-4 py-3 sm:flex-row sm:items-center">
      <div class="space-y-1.5">
        <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          YOUR LISTENING ROOM
        </div>
        <h2 id="spotlight-title" class="text-[22px] font-bold tracking-[-0.02em]">
          Find your next favorite.
        </h2>
        <p class="text-[13px] text-[var(--lw-text-secondary)]">
          Explore YouTube Music or add your local collection.
        </p>
      </div>
      <div class="flex flex-wrap gap-2.5">
        <button
          type="button"
          onclick={() => router.navigate('/search')}
          class="lw-press inline-flex h-9 cursor-pointer items-center gap-2 rounded-full bg-[var(--lw-accent)] px-5 text-[13px] font-bold text-[var(--lw-on-accent)]"
        >
          <WaveIcon name="search" size={14} /> Find music
        </button>
        <button
          type="button"
          onclick={() => router.navigate('/settings/library')}
          class="lw-press inline-flex h-9 cursor-pointer items-center gap-2 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface-overlay)] px-4 text-[13px] font-semibold text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="folder" size={14} /> Add folder
        </button>
      </div>
    </div>
  {/if}
</section>
