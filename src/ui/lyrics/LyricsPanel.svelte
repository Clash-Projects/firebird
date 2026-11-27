<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { featuresStore, LYRICS_PROVIDERS } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import LyricsShareCard from './LyricsShareCard.svelte';

  let shareText = $state<string | null>(null);
  let track = $derived(playerStore.currentTrack);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);
  const providerOptions = LYRICS_PROVIDERS.map(({ id, label }) => ({ value: id, label }));

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
  dataLw="lyrics-panel"
  class="flex h-full min-h-0 w-[min(380px,100vw)] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] shadow-[var(--lw-shadow-panel)]"
>
  <div class="shrink-0 space-y-2 border-b border-[var(--lw-divider)] px-4 py-3">
    <div class="flex items-center justify-between">
      <h2 class="text-[14px] font-bold">Lyrics</h2>
      <div class="flex items-center gap-1">
        <button
          type="button"
          onclick={() => (featuresStore.showRomanized = !featuresStore.showRomanized)}
          aria-pressed={featuresStore.showRomanized}
          use:waveTooltip={'Toggle romanization'}
          class="flex h-7 cursor-pointer items-center gap-1 rounded-[6px] px-2 text-[11px] font-semibold hover:bg-[var(--lw-wash-hover)] {featuresStore.showRomanized ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}"
        >
          Romaji
        </button>
        <button
          type="button"
          onclick={() => (featuresStore.showTranslation = !featuresStore.showTranslation)}
          aria-pressed={featuresStore.showTranslation}
          use:waveTooltip={'Toggle translation'}
          class="flex h-7 cursor-pointer items-center gap-1 rounded-[6px] px-2 text-[11px] font-semibold hover:bg-[var(--lw-wash-hover)] {featuresStore.showTranslation ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}"
        >
          <WaveIcon name="globe" size={13} /> Translate
        </button>
        <button
          type="button"
          use:waveTooltip={'Close lyrics'}
          onclick={() => windowState.closeRightPanel()}
          aria-label="Close lyrics"
          class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="close" size={15} />
        </button>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <div class="lyrics-provider min-w-0 flex-1">
        <Select
          label="Lyrics provider"
          value={featuresStore.lyricsProviderId}
          options={providerOptions}
          onchange={changeProvider}
          class="w-full"
        />
      </div>

      {#if track}
        <div class="flex h-9 shrink-0 items-center gap-0.5 rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-1 text-[11px] lw-tabular" role="group" aria-label="Lyrics timing offset">
          <button
            type="button"
            onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
            class="h-6 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            use:waveTooltip={'Delay -0.5s'}
          >−</button>
          <button
            type="button"
            onclick={() => featuresStore.resetLyricsOffset(track.id)}
            class="px-1 text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            use:waveTooltip={'Reset timing offset'}
          >{formatOffset(offsetMs)}</button>
          <button
            type="button"
            onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
            class="h-6 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
            use:waveTooltip={'Advance +0.5s'}
          >+</button>
        </div>
      {/if}
    </div>
  </div>

  <div use:waveSmoothScroll class="flex min-h-0 flex-1 flex-col px-3">
    <KaraokeLyricsView compact onShareLine={(text) => (shareText = text)} />
  </div>
</WaveHaze>

<LyricsShareCard open={shareText !== null} lyricText={shareText ?? ''} onClose={() => (shareText = null)} />

<style>
  .lyrics-provider :global(.lw-select) {
    --lw-select-picker-max-height: min(300px, 56dvh);
  }
  .lyrics-provider :global(.lw-select:focus-visible) {
    outline-width: 1px;
    outline-offset: -1px;
  }
</style>
