<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
</script>

<WaveHaze
  level="l2"
  dataLw="canvas-panel"
  class="flex h-full w-[360px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] p-4 shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between pb-3">
    <h2 class="text-[14px] font-bold">Canvas & Motion Art</h2>
    <button
      type="button"
      onclick={() => windowState.closeRightPanel()}
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>
  {#if track}
    <div class="relative flex-1 overflow-hidden rounded-[18px] border border-[var(--lw-outline)]">
      <AnimatedArtworkVideo posterUrl={track.artworkUrl} videoUrl={track.canvasUrl} radius={18} class="h-full w-full" />
      <div class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent p-4 text-white">
        <div class="text-[15px] font-bold">{track.title}</div>
        <div class="text-[12.5px] text-white/80">{track.artist}</div>
      </div>
    </div>
  {/if}
</WaveHaze>
