<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { syncThemeDocument } from '$src/features/settings/themeController.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let fileInput: HTMLInputElement | undefined = $state();
  let busy = $state(false);
  let status = $state('');
  let overwriteDialog = $state<HTMLDialogElement | null>(null);
  let pendingContent = $state('');
  let newerPlaylistNames = $state('');

  async function exportBackup() {
    if (busy) return;
    busy = true;
    status = '';
    try {
      const payload = isTauriRuntime()
        ? (await ipcInvoke<string>('library_backup_export')) ?? ''
        : JSON.stringify({ playlists: libraryStore.playlists, likedIds: libraryStore.likedIds }, null, 2);
      if (!payload) {
        status = 'Backup export failed.';
        return;
      }
      const blob = new Blob([payload], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'lastwave-backup.json';
      a.click();
      URL.revokeObjectURL(url);
      status = 'Saved backup.';
      featuresStore.showToast({ title: 'Saved backup.', severity: 'success' });
    } catch (err) {
      status = err instanceof Error ? err.message : 'Backup export failed.';
    } finally {
      busy = false;
    }
  }

  async function applyRestore(content: string, overwriteNewer: boolean) {
    busy = true;
    try {
      await ipcInvoke<number>('library_backup_restore_file', {
        content,
        overwriteNewer,
      });
      await Promise.allSettled([
        libraryStore.loadLocalStorage(),
        settingsStore.loadFromBackend(),
      ]);
      syncThemeDocument();
      status = overwriteNewer
        ? 'Backup restored, including newer local playlists.'
        : 'Backup merged. Newer local playlists were kept.';
    } catch (err) {
      status = err instanceof Error ? err.message : 'Could not restore backup.';
    } finally {
      busy = false;
      pendingContent = '';
    }
  }

  async function handleRestoreFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !isTauriRuntime() || busy) return;

    status = '';
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
        status = preview?.invalidReason ?? 'That file is not a LastWave backup.';
        return;
      }
      const newer = Array.isArray(preview.newerLocalTitles) ? preview.newerLocalTitles : [];
      if (newer.length > 0) {
        pendingContent = content;
        newerPlaylistNames = newer.join(', ');
        overwriteDialog?.showModal();
        return;
      }
      await applyRestore(content, false);
    } catch (err) {
      status = err instanceof Error ? err.message : 'That file is not a LastWave backup.';
    }
  }
</script>

<div>
  <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">Backup</div>
  <p class="mt-1 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
    Playlists, hidden recommendations, home layout and settings. Accounts, cookies and keys are never included.
  </p>

  <input
    bind:this={fileInput}
    type="file"
    accept=".json,application/json"
    class="hidden"
    onchange={(e) => void handleRestoreFile(e)}
  />

  <div class="mt-2.5 flex flex-wrap gap-2">
    <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void exportBackup()}>
      Export backup
    </Buttons>
    <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => fileInput?.click()}>
      Import backup
    </Buttons>
  </div>

  {#if status}
    <p class="mt-2 text-[12px] text-[var(--lw-text-secondary)]">{status}</p>
  {/if}
</div>

<dialog
  bind:this={overwriteDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => overwriteDialog?.close()}
  aria-label="Import backup"
  class="w-[min(440px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Import backup</h2>
  <p class="mt-2 text-[13px] leading-relaxed text-[var(--lw-text-secondary)]">
    These playlists are newer on this device and will be kept unless you overwrite them: {newerPlaylistNames}
  </p>
  <div class="mt-5 flex justify-end gap-2">
    <Buttons
      variant="secondary"
      size="sm"
      onclick={() => {
        const c = pendingContent;
        overwriteDialog?.close();
        if (c) void applyRestore(c, false);
      }}
    >
      Keep newer
    </Buttons>
    <Buttons
      variant="primary"
      size="sm"
      onclick={() => {
        const c = pendingContent;
        overwriteDialog?.close();
        if (c) void applyRestore(c, true);
      }}
    >
      Overwrite
    </Buttons>
  </div>
</dialog>
