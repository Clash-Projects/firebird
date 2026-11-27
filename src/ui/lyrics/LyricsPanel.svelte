<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import LyricsShareCard from './LyricsShareCard.svelte';

  let shareText = $state<string | null>(null);
</script>

<WaveHaze
  level="l2"
  class="flex h-full w-[380px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between border-b border-[var(--lw-divider)] px-4 py-3">
    <h2 class="text-[14px] font-bold">Lyrics</h2>
    <div class="flex items-center gap-1">
      <button
        type="button"
        onclick={() => (featuresStore.showTranslation = !featuresStore.showTranslation)}
        title="Toggle translation"
        class="flex h-7 cursor-pointer items-center gap-1 rounded-[8px] px-2 text-[11.5px] font-semibold hover:bg-[var(--lw-wash-hover)] {featuresStore.showTranslation ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}"
      >
        <WaveIcon name="globe" size={14} /> Translate
      </button>
      <button
        type="button"
        onclick={() => windowState.closeRightPanel()}
        class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="close" size={15} />
      </button>
    </div>
  </div>

  <div class="lw-scroll flex-1 px-3">
    <KaraokeLyricsView compact onShareLine={(text) => (shareText = text)} />
  </div>
</WaveHaze>

<LyricsShareCard open={shareText !== null} lyricText={shareText ?? ''} onClose={() => (shareText = null)} />
