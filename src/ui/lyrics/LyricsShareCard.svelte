<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  interface Props {
    open: boolean;
    lyricText: string;
    onClose: () => void;
  }

  let { open, lyricText, onClose }: Props = $props();
  let dialogEl = $state<HTMLDialogElement | null>(null);
  let track = $derived(playerStore.currentTrack);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) dialogEl.showModal();
    else if (!open && dialogEl.open) dialogEl.close();
  });

  function copyQuote() {
    const line = `"${lyricText}" — ${track?.artist ?? ''} (${track?.title ?? ''})`;
    navigator.clipboard?.writeText(line).catch(() => {});
    featuresStore.showToast({ title: 'Lyric quote copied', subtitle: track?.title, severity: 'success' });
    onClose();
  }
</script>

<dialog bind:this={dialogEl} closedby="any" onclose={onClose} aria-labelledby="share-lyric-title" class="lw-dialog w-full max-w-[420px]">
  <div class="p-6">
    <h1 id="share-lyric-title" class="text-[16px] font-bold">Share Lyric Card</h1>
    <div
      class="mt-4 rounded-[20px] p-6 text-white"
      style:background="linear-gradient(135deg, {track?.accentColor ?? '#2d1e4f'}, #0e0e0e)"
    >
      <p class="text-[20px] font-bold leading-snug">“{lyricText}”</p>
      {#if track}
        <div class="mt-5 flex items-center gap-3">
          <Artwork src={track.artworkUrl} size={40} radius={8} />
          <div class="min-w-0">
            <div class="truncate text-[13px] font-semibold">{track.title}</div>
            <div class="truncate text-[12px] text-white/75">{track.artist}</div>
          </div>
        </div>
      {/if}
    </div>
    <div class="mt-4 flex justify-end gap-2">
      <Buttons variant="ghost" onclick={onClose}>Close</Buttons>
      <Buttons variant="primary" onclick={copyQuote}>Copy Quote</Buttons>
    </div>
  </div>
</dialog>
