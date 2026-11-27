<script lang="ts">
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let fileInput: HTMLInputElement | undefined = $state();
  let busy = $state(false);

  async function exportBackup() {
    if (busy) return;
    busy = true;
    try {
      const payload = isTauriRuntime()
        ? (await ipcInvoke<string>('library_backup_export')) ?? ''
        : JSON.stringify({ playlists: libraryStore.playlists, likedIds: libraryStore.likedIds }, null, 2);
      if (!payload) {
        featuresStore.showToast({ title: 'Backup export failed', severity: 'error' });
        return;
      }
      const blob = new Blob([payload], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `lastwave-backup-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      featuresStore.showToast({ title: 'Backup exported', subtitle: a.download, severity: 'success' });
    } finally {
      busy = false;
    }
  }

  async function handleRestoreFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !isTauriRuntime() || busy) return;

    busy = true;
    try {
      const content = await file.text();
      const preview = await ipcInvoke<{
        valid: boolean;
        invalidReason?: string | null;
        playlistCount: number;
        exclusionCount: number;
        newerLocalTitles?: string[];
      }>('library_backup_preview', { content });
      if (!preview?.valid) {
        featuresStore.showToast({
          title: 'Invalid backup file',
          subtitle: preview?.invalidReason ?? 'That file is not a LastWave backup.',
          severity: 'error',
        });
        return;
      }
      const hasNewer = Array.isArray(preview.newerLocalTitles) && preview.newerLocalTitles.length > 0;
      const restored = await ipcInvoke<number>('library_backup_restore_file', {
        content,
        overwriteNewer: !hasNewer,
      });
      await Promise.allSettled([
        libraryStore.loadLocalStorage(),
        settingsStore.loadFromBackend(),
      ]);
      featuresStore.showToast({
        title: 'Backup restored',
        subtitle: `${restored ?? preview.playlistCount} playlists · ${preview.exclusionCount} exclusions`,
        severity: 'success',
      });
    } catch (err) {
      featuresStore.showToast({
        title: 'Restore failed',
        subtitle: err instanceof Error ? err.message : 'Could not restore backup',
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Settings & Playlist Backup</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">Export or restore your liked songs, playlists, and preferences.</p>
  </div>
  <div class="flex gap-2">
    <input
      bind:this={fileInput}
      type="file"
      accept=".json,application/json"
      class="hidden"
      onchange={(e) => void handleRestoreFile(e)}
    />
    <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => fileInput?.click()}>
      Restore JSON
    </Buttons>
    <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void exportBackup()}>
      {busy ? 'Working...' : 'Export JSON'}
    </Buttons>
  </div>
</div>
