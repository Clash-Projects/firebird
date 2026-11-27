<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
</script>

<div class="flex h-full w-full items-center justify-center bg-[var(--lw-bg-deep)] p-6">
  <WaveHaze
    level="l2"
    dataLw="mini-player"
    class="w-[340px] rounded-[20px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-4 shadow-[var(--lw-shadow-artwork)]"
  >
    <div class="flex items-center justify-between pb-3">
      <span class="text-[11px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">Mini Player</span>
      <button
        type="button"
        onclick={() => windowState.toggleMiniPlayer()}
        class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        title="Return to full player"
      >
        <WaveIcon name="maximize" size={14} />
      </button>
    </div>

    {#if track}
      <div class="flex items-center gap-3.5">
        <Artwork src={track.artworkUrl} alt={track.title} size={64} radius={12} shadow="lift" />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[14px] font-bold">{track.title}</div>
          <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{track.artist}</div>
          <div class="mt-1 text-[10.5px] text-[var(--lw-text-tertiary)]">
            {track.codec} · {track.bitDepth}-bit / {(track.sampleRateHz / 1000).toFixed(1)} kHz
          </div>
        </div>
      </div>

      <div class="mt-4 flex items-center gap-2">
        <span class="text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">{formatDuration(playerStore.positionSec)}</span>
        <input
          type="range"
          min="0"
          max={playerStore.durationSec || 1}
          value={playerStore.positionSec}
          oninput={(e) => playerStore.seek(Number((e.currentTarget as HTMLInputElement).value))}
          class="h-1 flex-1 cursor-pointer accent-[var(--lw-accent)]"
        />
        <span class="text-[11px] text-[var(--lw-text-tertiary)] lw-tabular">{formatDuration(playerStore.durationSec)}</span>
      </div>

      <div class="mt-3 flex items-center justify-center gap-3">
        <button
          type="button"
          onclick={() => playerStore.previous()}
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="prev" size={16} />
        </button>
        <button
          type="button"
          onclick={() => playerStore.togglePlay()}
          class="flex h-10 w-10 cursor-pointer items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)]"
        >
          <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={18} filled />
        </button>
        <button
          type="button"
          onclick={() => playerStore.next()}
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="next" size={16} />
        </button>
      </div>
    {/if}
  </WaveHaze>
</div>
