<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtTrack } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let ytTracks = $state<Track[]>([]);
  let loading = $state(false);

  async function loadHistory() {
    if (!isTauriRuntime() || !settingsStore.ytConnected || loading) return;
    loading = true;
    try {
      const rows = await ipcInvoke<Parameters<typeof mapYtTrack>[0][]>('yt_fetch_history', {
        limit: 100,
      });
      if (Array.isArray(rows)) {
        const mapped = rows.map(mapYtTrack);
        libraryStore.upsertTracks(mapped);
        libraryStore.historyIds = mapped.map((t) => t.id);
        ytTracks = mapped;
      }
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadHistory();
  });

  let tracks = $derived(
    ytTracks.length > 0 ? ytTracks : libraryStore.tracksByIds(libraryStore.historyIds)
  );
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Listening History" subtitle="Recently played tracks">
    {#snippet actions()}
      {#if settingsStore.ytConnected}
        <Buttons variant="secondary" size="sm" disabled={loading} onclick={() => void loadHistory()}>
          <WaveIcon name="refresh" size={14} class={loading ? 'animate-spin' : ''} />
          <span>{loading ? 'Refreshing...' : 'Refresh'}</span>
        </Buttons>
      {/if}
    {/snippet}
  </CommandBar>
  <DesktopTable {tracks} />
</div>
