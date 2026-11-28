<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
</script>

<WaveHaze
  level="l2"
  dataLw="lyrics-panel"
  class="flex h-full min-h-0 w-[min(392px,100vw)] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <KaraokeLyricsView
    compact
    showHeaderControls={true}
    onClose={() => windowState.closeRightPanel()}
  />
</WaveHaze>
