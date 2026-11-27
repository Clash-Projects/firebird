<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  function exportBackup() {
    const payload = JSON.stringify({ playlists: libraryStore.playlists, likedIds: libraryStore.likedIds }, null, 2);
    const blob = new Blob([payload], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'lastwave-backup.json';
    a.click();
    URL.revokeObjectURL(url);
    featuresStore.showToast({ title: 'Backup exported', subtitle: 'lastwave-backup.json', severity: 'success' });
  }
</script>

<div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Settings & Playlist Backup</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">Export or restore your liked songs, playlists, and preferences.</p>
  </div>
  <div class="flex gap-2">
    <Buttons variant="secondary" size="sm" onclick={exportBackup}>Export JSON</Buttons>
  </div>
</div>
