<script lang="ts">
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
</script>

{#if track}
  <div class="flex h-full flex-col items-center justify-center px-6 py-8 text-center">
    <Artwork src={track.artworkUrl} alt={track.title} size={300} radius={24} shadow="artwork" />
    <h1 class="mt-6 text-[24px] font-bold tracking-[-0.4px]">{track.title}</h1>
    <p class="mt-1 text-[14px] text-[var(--lw-text-secondary)]">{track.artist}</p>

    <div class="mt-6 flex w-full max-w-[440px] items-center gap-3">
      <span class="text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{formatDuration(playerStore.positionSec)}</span>
      <input
        type="range"
        min="0"
        max={playerStore.durationSec || 1}
        value={playerStore.positionSec}
        oninput={(e) => playerStore.seek(Number((e.currentTarget as HTMLInputElement).value))}
        class="h-1.5 flex-1 cursor-pointer accent-[var(--lw-accent)]"
      />
      <span class="text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{formatDuration(playerStore.durationSec)}</span>
    </div>

    <div class="mt-5 flex items-center gap-4">
      <button type="button" onclick={() => playerStore.previous()} class="flex h-10 w-10 cursor-pointer items-center justify-center rounded-[10px] hover:bg-[var(--lw-wash-hover)]">
        <WaveIcon name="prev" size={20} />
      </button>
      <button type="button" onclick={() => playerStore.togglePlay()} class="flex h-12 w-12 cursor-pointer items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)] shadow-[var(--lw-shadow-transport)]">
        <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={20} filled />
      </button>
      <button type="button" onclick={() => playerStore.next()} class="flex h-10 w-10 cursor-pointer items-center justify-center rounded-[10px] hover:bg-[var(--lw-wash-hover)]">
        <WaveIcon name="next" size={20} />
      </button>
    </div>
  </div>
{/if}
