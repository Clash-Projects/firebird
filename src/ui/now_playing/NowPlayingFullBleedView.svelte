<script lang="ts">
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import AmbientStage from '$src/ui/components/AmbientStage.svelte';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { immersiveState } from './immersive.svelte';
  import NowPlayingOverflowMenu from './NowPlayingOverflowMenu.svelte';

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let lyricsMissing = $derived(!featuresStore.lyricsLoading && (!doc || doc.lines.length === 0));

  let canvasVideoUrl = $state('');
  let lastTrackKey = '';

  $effect(() => {
    const t = track;
    if (!t) {
      canvasVideoUrl = '';
      lastTrackKey = '';
      return;
    }
    const key = `${t.id}|${t.title}|${t.artist}`;
    if (key === lastTrackKey) return;
    lastTrackKey = key;
    canvasVideoUrl = '';

    void (async () => {
      try {
        const spotify = await ipcInvoke<{ canvasUrl?: string; url?: string }>('spotify_canvas', {
          title: t.title,
          artist: t.artist,
          durationMs: Math.round((t.durationSec || 0) * 1000),
        });
        const spUrl = spotify?.canvasUrl || spotify?.url || '';
        if (lastTrackKey === key && spUrl) {
          canvasVideoUrl = spUrl;
          return;
        }
      } catch {
        // Fallback to animated artwork lookup
      }

      try {
        const anim = await ipcInvoke<{ url?: string; fallbackUrl?: string }>('artwork_lookup_animated', {
          artist: t.artist,
          album: t.album || '',
          title: t.title,
        });
        const url = anim?.url || anim?.fallbackUrl || '';
        if (lastTrackKey === key && url) {
          canvasVideoUrl = url;
        }
      } catch {
        // Use static cover art
      }
    })();
  });
</script>

{#if track}
  <div class="relative h-full w-full overflow-hidden">
    <!-- 0. Full-Stage Color-Adaptive Ambient Backdrop -->
    <AmbientStage color={track.accentColor} intensity={0.42} />

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
      <AnimatedArtworkVideo
        posterUrl={track.artworkUrl}
        videoUrl={canvasVideoUrl || undefined}
        radius={0}
        class="h-full w-full"
      />
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
            Lyrics not found
          </span>
        </div>
      {:else}
        <div use:waveSmoothScroll class="lw-scroll h-full min-h-0 flex-1 overflow-y-auto">
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
