<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let cacheSizeMb = $state(0);

  onMount(async () => {
    if (!isTauriRuntime()) return;
    const stats = await ipcInvoke<{ totalBytes?: number }>('library_artwork_cache_stats');
    if (stats && typeof stats.totalBytes === 'number') {
      cacheSizeMb = stats.totalBytes / (1024 * 1024);
    }
  });

  async function clearCache() {
    await Promise.all([
      ipcInvoke('library_artwork_cache_clear'),
      ipcInvoke('library_snapshot_cache_clear', { group: null }),
    ]);
    cacheSizeMb = 0;
    featuresStore.showToast({ title: 'Artwork & stream cache cleared', severity: 'success' });
  }
</script>

<div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Disk Cache</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)] lw-tabular">
      Currently using {cacheSizeMb.toFixed(1)} MB for artwork thumbnails and metadata snapshots.
    </p>
  </div>
  <Buttons variant="secondary" size="sm" onclick={() => void clearCache()}>Clear Cache</Buttons>
</div>
