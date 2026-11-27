<script lang="ts">
  import { formatDuration, playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
</script>

{#if track}
  <div class="relative flex h-full w-full flex-col justify-end overflow-hidden p-10">
    <img src={track.artworkUrl} alt="" class="pointer-events-none absolute inset-0 h-full w-full scale-110 object-cover opacity-35 blur-2xl" />
    <div class="pointer-events-none absolute inset-0 bg-gradient-to-t from-black/85 via-black/35 to-black/20"></div>

    <div class="relative z-10 flex flex-wrap items-end gap-8">
      <Artwork src={track.artworkUrl} alt={track.title} size={220} radius={24} shadow="artwork" />
      <div class="min-w-[260px] flex-1 space-y-3 text-white">
        <div class="text-[11px] font-bold tracking-[1.2px] text-white/65 uppercase">
          {track.codec} · {track.bitDepth}-bit / {(track.sampleRateHz / 1000).toFixed(1)} kHz
        </div>
        <h1 class="text-[36px] font-bold tracking-[-0.8px] leading-tight">{track.title}</h1>
        <p class="text-[16px] text-white/80">{track.artist} · {track.album}</p>

        <div class="flex max-w-[520px] items-center gap-3 pt-2">
          <span class="text-[12px] text-white/70 lw-tabular">{formatDuration(playerStore.positionSec)}</span>
          <input
            type="range"
            min="0"
            max={playerStore.durationSec || 1}
            value={playerStore.positionSec}
            oninput={(e) => playerStore.seek(Number((e.currentTarget as HTMLInputElement).value))}
            class="h-1.5 flex-1 cursor-pointer accent-white"
          />
          <span class="text-[12px] text-white/70 lw-tabular">{formatDuration(playerStore.durationSec)}</span>
        </div>

        <div class="flex items-center gap-3 pt-1">
          <button type="button" onclick={() => playerStore.previous()} class="flex h-10 w-10 cursor-pointer items-center justify-center rounded-full bg-white/10 hover:bg-white/20">
            <WaveIcon name="prev" size={18} />
          </button>
          <button type="button" onclick={() => playerStore.togglePlay()} class="flex h-12 w-12 cursor-pointer items-center justify-center rounded-full bg-white text-black">
            <WaveIcon name={playerStore.isPlaying ? 'pause' : 'play'} size={20} filled />
          </button>
          <button type="button" onclick={() => playerStore.next()} class="flex h-10 w-10 cursor-pointer items-center justify-center rounded-full bg-white/10 hover:bg-white/20">
            <WaveIcon name="next" size={18} />
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
