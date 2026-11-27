<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let cacheSizeMb = $state(148.4);

  function clearCache() {
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
  <Buttons variant="secondary" size="sm" onclick={clearCache}>Clear Cache</Buttons>
</div>
