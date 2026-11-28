<script lang="ts">
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import AmbientStage from '$src/ui/components/AmbientStage.svelte';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { immersiveState } from './immersive.svelte';
  import NowPlayingOverflowMenu from './NowPlayingOverflowMenu.svelte';

  interface Props {
    videoUrls: readonly string[];
    ambientArtworkUrl: string;
    onArtworkLoad: (url: string) => void;
  }

  let { videoUrls, ambientArtworkUrl, onArtworkLoad }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let lyricsMissing = $derived(
    !featuresStore.lyricsLoading &&
      (!doc || Boolean(doc.isInstrumental) || doc.lines.length === 0)
  );
  let isInstrumental = $derived(Boolean(doc?.isInstrumental));
</script>

{#if track}
  <div class="relative h-full w-full overflow-hidden">
    <!-- 0. Full-Stage Color-Adaptive Ambient Backdrop -->
    <AmbientStage artworkUrl={ambientArtworkUrl} intensity={0.42} />

    <!-- 1. Left Full-Bleed Canvas / Hi-Res Cover with Wide Smoothstep Feather -->
    <div
      class="pointer-events-none absolute top-0 bottom-0 left-0 overflow-hidden"
      style="
        width: clamp(240px, max(44%, 96vh), 52%);
        -webkit-mask-image: linear-gradient(
          to right,
          #fff 0%,
          #fff 40%,
          rgba(255, 255, 255, 0.92) 50%,
          rgba(255, 255, 255, 0.74) 60%,
          rgba(255, 255, 255, 0.48) 70%,
          rgba(255, 255, 255, 0.24) 80%,
          rgba(255, 255, 255, 0.07) 89%,
          transparent 95%,
          transparent 100%
        );
        mask-image: linear-gradient(
          to right,
          #fff 0%,
          #fff 40%,
          rgba(255, 255, 255, 0.92) 50%,
          rgba(255, 255, 255, 0.74) 60%,
          rgba(255, 255, 255, 0.48) 70%,
          rgba(255, 255, 255, 0.24) 80%,
          rgba(255, 255, 255, 0.07) 89%,
          transparent 95%,
          transparent 100%
        );
      "
    >
      {#key track.id}
        <AnimatedArtworkVideo
          posterUrl={track.artworkUrl}
          title={track.title}
          artist={track.artist}
          album={track.album}
          {onArtworkLoad}
          {videoUrls}
          radius={0}
          class="h-full w-full"
        />
      {/key}
    </div>

    <!-- 2. Right Pane: Lyrics (or clean auto-hiding "Lyrics not found" notice) -->
    <div
      class="absolute top-0 right-0 bottom-0 flex flex-col pt-[56px] pr-[36px] pb-[24px] pl-[20px]"
      style="left: min(44%, clamp(240px, max(44%, 96vh), 52%) * 0.90);"
    >
      {#if lyricsMissing}
        <div
          class="flex h-full items-center justify-center transition-opacity duration-350"
          style:opacity={immersiveState.controlsIdle ? 0 : 1}
        >
          <span class="text-[16px] font-medium tracking-[-0.1px] text-[var(--lw-text-secondary)]">
            {isInstrumental ? 'Instrumental' : 'Lyrics not found'}
          </span>
        </div>
      {:else}
        <div class="h-full min-h-0 flex-1 overflow-hidden">
          <KaraokeLyricsView fontSize={34} showHeaderControls={false} />
        </div>
      {/if}
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
            void immersiveState.setImmersive(false);
          }}
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-primary)] transition hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="chevronLeft" size={15} />
        </button>
      </div>
    {/if}

    <!-- 4. Unified Top-Right Control Bar (fades on idle) -->
    <div
      class="absolute top-[18px] right-[20px] z-30 transition-opacity duration-350"
      class:pointer-events-none={immersiveState.controlsIdle}
      style:opacity={immersiveState.controlsIdle ? 0 : 1}
    >
      <NowPlayingOverflowMenu showAmbientToggle={false} />
    </div>
  </div>
{/if}
