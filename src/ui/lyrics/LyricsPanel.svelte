<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { featuresStore, LYRICS_PROVIDERS } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import LyricsShareCard from './LyricsShareCard.svelte';

  let shareText = $state<string | null>(null);
  let track = $derived(playerStore.currentTrack);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  function formatOffset(ms: number): string {
    if (ms === 0) return '0.0s';
    const sign = ms > 0 ? '+' : '-';
    return `${sign}${(Math.abs(ms) / 1000).toFixed(1)}s`;
  }

  function changeProvider(providerId: string) {
    featuresStore.lyricsProviderId = providerId;
    if (track) {
      void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true, providerId });
    }
  }
</script>

<WaveHaze
  level="l2"
  class="flex h-full w-[380px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <div class="space-y-2 border-b border-[var(--lw-divider)] px-4 py-3">
    <div class="flex items-center justify-between">
      <h2 class="text-[14px] font-bold">Lyrics</h2>
      <div class="flex items-center gap-1">
        <button
          type="button"
          onclick={() => (featuresStore.showRomanized = !featuresStore.showRomanized)}
          title="Toggle romanization"
          class="flex h-7 cursor-pointer items-center gap-1 rounded-[8px] px-2 text-[11px] font-semibold hover:bg-[var(--lw-wash-hover)] {featuresStore.showRomanized ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}"
        >
          Romaji
        </button>
        <button
          type="button"
          onclick={() => (featuresStore.showTranslation = !featuresStore.showTranslation)}
          title="Toggle translation"
          class="flex h-7 cursor-pointer items-center gap-1 rounded-[8px] px-2 text-[11px] font-semibold hover:bg-[var(--lw-wash-hover)] {featuresStore.showTranslation ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}"
        >
          <WaveIcon name="globe" size={13} /> Translate
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

    <div class="flex items-center justify-between gap-2">
      <select
        value={featuresStore.lyricsProviderId}
        onchange={(e) => changeProvider(e.currentTarget.value)}
        aria-label="Lyrics provider"
        class="h-7 flex-1 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-2 text-[11.5px] text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] focus:outline-none"
      >
        {#each LYRICS_PROVIDERS as p (p.id)}
          <option value={p.id}>{p.label}</option>
        {/each}
      </select>

      {#if track}
        <div class="flex items-center gap-0.5 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-1 text-[11px] lw-tabular">
          <button
            type="button"
            onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
            class="h-6 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            title="Delay -0.5s"
          >−</button>
          <button
            type="button"
            onclick={() => featuresStore.resetLyricsOffset(track.id)}
            class="px-1 text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            title="Reset timing offset"
          >{formatOffset(offsetMs)}</button>
          <button
            type="button"
            onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
            class="h-6 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            title="Advance +0.5s"
          >+</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="lw-scroll flex-1 px-3">
    <KaraokeLyricsView compact onShareLine={(text) => (shareText = text)} />
  </div>
</WaveHaze>

<LyricsShareCard open={shareText !== null} lyricText={shareText ?? ''} onClose={() => (shareText = null)} />
