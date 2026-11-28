<script lang="ts">
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let scanning = $state(false);

  async function pickFolder() {
    if (!isTauriRuntime()) return;
    try {
      const picked = await ipcInvoke<string | null>('dialog_pick_folder', {
        title: 'Choose music folder',
      });
      if (picked && picked.trim()) {
        libraryStore.addLocalFolder(picked.trim());
      }
    } catch {
      featuresStore.showToast({ title: 'Could not open folder picker', severity: 'warning' });
    }
  }

  async function rescanAll() {
    if (!isTauriRuntime() || scanning) return;
    scanning = true;
    try {
      const count = await ipcInvoke<number>('library_local_rescan');
      await libraryStore.loadLocalStorage();
      featuresStore.showToast({
        title: 'Library rescanned',
        subtitle: typeof count === 'number' ? `${count} local tracks indexed` : undefined,
        severity: 'success',
      });
    } finally {
      scanning = false;
    }
  }
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center gap-2">
    <Buttons variant="primary" size="sm" onclick={() => void pickFolder()}>
      <WaveIcon name="folderOpen" size={14} />
      Add folder
    </Buttons>
    {#if libraryStore.localFolders.length > 0}
      <Buttons variant="secondary" size="sm" disabled={scanning} onclick={() => void rescanAll()}>
        <WaveIcon name="refresh" size={14} class={scanning ? 'animate-spin' : ''} />
        {scanning ? 'Scanning…' : 'Rescan'}
      </Buttons>
    {/if}
  </div>

  {#if libraryStore.localFolders.length === 0}
    <p class="text-[12px] text-[var(--lw-text-tertiary)]">
      No local folders added yet. Add a folder to index FLAC, ALAC, WAV, MP3, and Opus files.
    </p>
  {:else}
    <div class="space-y-1.5">
      {#each libraryStore.localFolders as folder (folder)}
        <div class="flex items-center justify-between gap-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5">
          <div class="flex min-w-0 items-center gap-2.5">
            <WaveIcon name="folderOpen" size={15} class="shrink-0 text-[var(--lw-text-secondary)]" />
            <span class="truncate text-[13px]">{folder}</span>
          </div>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => libraryStore.removeLocalFolder(folder)}
          >
            Forget
          </Buttons>
        </div>
      {/each}
    </div>
  {/if}
</div>
