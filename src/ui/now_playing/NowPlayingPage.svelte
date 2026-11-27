<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import AmbientStage from '$src/ui/components/AmbientStage.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import NowPlayingFullBleedView from './NowPlayingFullBleedView.svelte';
  import NowPlayingMinimalView from './NowPlayingMinimalView.svelte';
  import NowPlayingOverflowMenu from './NowPlayingOverflowMenu.svelte';

  let track = $derived(playerStore.currentTrack);
  let nextTrack = $derived(playerStore.queue[playerStore.currentIndex + 1] ?? null);
  let lyricsVisible = $state(true);
  let focusMode = $state(false);
</script>

<div data-lw="now-playing" class="relative flex h-full w-full flex-col overflow-hidden bg-[var(--lw-bg)]">
  <AmbientStage color={track?.accentColor} intensity={0.38} />

  <div class="relative z-20 flex items-center justify-end gap-2 px-6 pt-4">
    <button
      type="button"
      onclick={() => {
        lyricsVisible = !lyricsVisible;
        if (!lyricsVisible) focusMode = false;
      }}
      class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[14px] border border-white/15 bg-black/35 px-3 text-[12px] font-semibold backdrop-blur-md transition-colors hover:bg-black/55 {lyricsVisible
        ? 'text-[var(--lw-accent)]'
        : 'text-[var(--lw-text-secondary)]'}"
    >
      <WaveIcon name="lyrics" size={14} /> Lyrics
    </button>

    {#if lyricsVisible}
      <button
        type="button"
        onclick={() => (focusMode = !focusMode)}
        class="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[14px] border border-white/15 bg-black/35 px-3 text-[12px] font-semibold backdrop-blur-md transition-colors hover:bg-black/55 {focusMode
          ? 'text-[var(--lw-accent)]'
          : 'text-[var(--lw-text-secondary)]'}"
      >
        Focus
      </button>
    {/if}

    <NowPlayingOverflowMenu />

    <button
      type="button"
      onclick={() => router.back()}
      title="Close Now Playing"
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[14px] border border-white/15 bg-black/35 text-[var(--lw-text-primary)] backdrop-blur-md hover:bg-black/55"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  <div class="relative z-10 min-h-0 flex-1">
    {#if settingsStore.nowPlayingStyle === 'fullBleed'}
      <NowPlayingFullBleedView />
    {:else if settingsStore.nowPlayingStyle === 'minimal'}
      <NowPlayingMinimalView />
    {:else if track}
      <div
        class="mx-auto flex h-full max-w-[1220px] items-center justify-center gap-14 px-10 pb-6 transition-all duration-[260ms]"
      >
        {#if !focusMode}
          <div class="flex flex-col items-center {lyricsVisible ? 'w-[420px] shrink-0 lg:items-start' : 'items-center text-center'}">
            <Artwork
              src={track.artworkUrl}
              alt={track.title}
              size={lyricsVisible ? 360 : 400}
              radius={24}
              shadow="artwork"
            />
            <div class="mt-6 w-full max-w-[380px]">
              <h1 class="truncate text-[24px] font-bold tracking-[-0.5px]">{track.title}</h1>
              <p class="mt-0.5 truncate text-[15px] text-[var(--lw-text-secondary)]">{track.artist}</p>
              <div class="mt-1 text-[12px] text-[var(--lw-text-tertiary)]">
                {track.album} · {track.codec} {track.bitDepth}-bit / {(track.sampleRateHz / 1000).toFixed(0)} kHz
              </div>
              {#if nextTrack}
                <div class="mt-4 inline-flex items-center gap-2 rounded-[12px] border border-white/10 bg-white/[0.05] px-3 py-1.5 text-[11.5px] text-[var(--lw-text-secondary)]">
                  <span class="font-bold tracking-[0.6px] text-[var(--lw-text-tertiary)] uppercase">Up Next</span>
                  <span class="truncate text-[var(--lw-text-primary)]">{nextTrack.title}</span>
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if lyricsVisible}
          <div class="lw-scroll h-full max-h-[580px] flex-1 pr-2">
            <KaraokeLyricsView />
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>
