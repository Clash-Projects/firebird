<script lang="ts">
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import AmbientStage from '$src/ui/components/AmbientStage.svelte';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { immersiveState } from './immersive.svelte';
  import NowPlayingOverflowMenu from './NowPlayingOverflowMenu.svelte';

  interface Props {
    lyricsVisible?: boolean;
    videoUrls: readonly string[];
    onToggleLyrics?: () => void;
    onClose?: () => void;
  }

  let { lyricsVisible = true, videoUrls, onToggleLyrics, onClose }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let lyricsMissing = $derived(!featuresStore.lyricsLoading && (!doc || doc.lines.length === 0));
  let showLyrics = $derived(lyricsVisible && !lyricsMissing);
  let ambientEnabled = $state(true);
</script>

{#if track}
  <div class="relative h-full w-full overflow-hidden">
    <!-- 1. Backdrop -->
    {#if ambientEnabled}
      <AmbientStage color={track.accentColor} intensity={0.38} />
    {/if}

    <!-- 2. Main Layout: Centers cover smoothly when lyrics are hidden or unavailable -->
    <div class="relative z-10 flex h-full w-full items-center justify-center px-[56px] py-[40px]">
      <div
        class="flex h-full w-full items-center justify-center transition-all duration-[420ms] ease-[cubic-bezier(0.4,0,0.2,1)]"
        style:gap={showLyrics ? '48px' : '0px'}
      >
        <!-- Cover + Centered Track Info -->
        <div
          class="flex shrink-0 flex-col items-center transition-all duration-[420ms] ease-[cubic-bezier(0.4,0,0.2,1)]"
          style:width={showLyrics ? 'min(42%, 440px)' : 'min(100%, 440px)'}
        >
          <div
            class="aspect-square w-full overflow-hidden rounded-[2px] shadow-[var(--lw-shadow-artwork)] transition-all duration-[420ms] ease-[cubic-bezier(0.4,0,0.2,1)]"
            style:max-width={showLyrics ? '420px' : '440px'}
          >
            {#key track.id}
              <AnimatedArtworkVideo
                posterUrl={track.artworkUrl}
                title={track.title}
                artist={track.artist}
                album={track.album}
                videoUrls={ambientEnabled ? videoUrls : []}
                radius={2}
                class="h-full w-full"
              />
            {/key}
          </div>

          <h1 class="mt-[18px] line-clamp-2 text-center text-[18px] font-bold tracking-[-0.2px] text-[var(--lw-text-primary)]">
            {track.title}
          </h1>
          <p class="mt-1 truncate text-center text-[13.5px] text-[var(--lw-text-secondary)]">
            {track.artist}
          </p>
        </div>

        <!-- Right Column: Synced Lyrics (collapses smoothly when hidden or not found) -->
        <div
          class="h-full min-h-0 overflow-hidden transition-all duration-[420ms] ease-[cubic-bezier(0.4,0,0.2,1)]"
          style:width={showLyrics ? '54%' : '0px'}
          style:opacity={showLyrics ? 1 : 0}
          class:pointer-events-none={!showLyrics}
        >
          <div use:waveSmoothScroll class="lw-scroll h-full min-w-[320px] overflow-y-auto">
            <KaraokeLyricsView fontSize={34} showHeaderControls={false} />
          </div>
        </div>
      </div>
    </div>

    <!-- 3. Top-Left Exit Fullscreen button (only in immersive fullscreen, fades on idle) -->
    {#if immersiveState.isImmersive}
      <div
        class="absolute top-[18px] left-[20px] z-30 transition-opacity duration-350"
        class:pointer-events-none={immersiveState.controlsIdle}
        style:opacity={immersiveState.controlsIdle ? 0 : 1}
      >
        <button
          type="button"
          use:waveTooltip={'Exit fullscreen (F11)'}
          aria-label="Exit fullscreen"
          onclick={() => {
            immersiveState.ping();
            onClose?.();
          }}
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-primary)] transition hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="chevronLeft" size={15} />
        </button>
      </div>
    {/if}

    <!-- 4. Unified Top-Right Control Bar (Ambient, Fullscreen, Overflow — fades on idle) -->
    <div
      class="absolute top-[18px] right-[20px] z-30 transition-opacity duration-350"
      class:pointer-events-none={immersiveState.controlsIdle}
      style:opacity={immersiveState.controlsIdle ? 0 : 1}
    >
      <NowPlayingOverflowMenu
        showAmbientToggle={true}
        {ambientEnabled}
        onToggleAmbient={() => (ambientEnabled = !ambientEnabled)}
        {lyricsVisible}
        {onToggleLyrics}
      />
    </div>
  </div>
{/if}
