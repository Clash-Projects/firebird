<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let query = $state('');

  $effect(() => {
    if (!dialogEl) return;
    if (windowState.commandPaletteOpen && !dialogEl.open) {
      query = '';
      dialogEl.showModal();
    } else if (!windowState.commandPaletteOpen && dialogEl.open) {
      dialogEl.close();
    }
  });

  const quickActions = [
    { label: 'Go to Home', action: () => router.navigate('/home') },
    { label: 'Open Now Playing', action: () => router.navigate('/now-playing') },
    { label: 'Open Equalizer', action: () => windowState.toggleRightPanel('eq') },
    { label: 'Open Audio Output', action: () => windowState.toggleRightPanel('output') },
    { label: 'Open Plugins & Themes', action: () => router.navigate('/settings/plugins') },
    { label: 'Toggle Play / Pause', action: () => playerStore.togglePlay() },
    { label: 'Toggle Mini Player', action: () => windowState.toggleMiniPlayer() },
  ];

  let matchedTracks = $derived(
    query.trim()
      ? libraryStore.tracks.filter(
          (t) =>
            t.title.toLowerCase().includes(query.toLowerCase()) ||
            t.artist.toLowerCase().includes(query.toLowerCase())
        )
      : libraryStore.tracks.slice(0, 4)
  );

  let matchedActions = $derived(
    query.trim()
      ? quickActions.filter((a) => a.label.toLowerCase().includes(query.toLowerCase()))
      : quickActions
  );

  function close() {
    windowState.commandPaletteOpen = false;
  }

  function run(fn: () => void) {
    fn();
    close();
  }
</script>

<dialog
  bind:this={dialogEl}
  data-lw="command-palette"
  onclose={close}
  aria-label="Command palette"
  class="w-full max-w-[540px] overflow-hidden"
>
  <div class="flex items-center gap-2.5 border-b border-[var(--lw-divider)] px-4 py-3">
    <WaveIcon name="search" size={16} class="text-[var(--lw-text-tertiary)]" />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="text"
      autofocus
      bind:value={query}
      placeholder="Jump to track, page, or command..."
      class="flex-1 bg-transparent text-[13.5px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] focus:outline-none"
    />
    <button
      type="button"
      onclick={close}
      class="cursor-pointer rounded-[6px] px-1.5 py-0.5 text-[11px] text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)]"
    >
      ESC
    </button>
  </div>

  <div class="lw-scroll max-h-[380px] p-2">
    {#if matchedActions.length > 0}
      <div class="px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        Commands
      </div>
      {#each matchedActions as item (item.label)}
        <button
          type="button"
          onclick={() => run(item.action)}
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-2.5 py-2 text-left text-[13px] hover:bg-[var(--lw-wash-hover)]"
        >
          <WaveIcon name="terminal" size={14} class="text-[var(--lw-text-tertiary)]" />
          <span>{item.label}</span>
        </button>
      {/each}
    {/if}

    {#if matchedTracks.length > 0}
      <div class="mt-2 px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        Tracks
      </div>
      {#each matchedTracks as track (track.id)}
        <button
          type="button"
          onclick={() => run(() => playTrackNow(track))}
          class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <Artwork src={track.artworkUrl} size={32} radius={6} />
          <div class="min-w-0 flex-1">
            <div class="truncate text-[13px] font-semibold">{track.title}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{track.artist}</div>
          </div>
          <span class="text-[11px] text-[var(--lw-text-tertiary)]">{track.codec}</span>
        </button>
      {/each}
    {/if}
  </div>
</dialog>
