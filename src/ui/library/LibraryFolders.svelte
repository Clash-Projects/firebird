<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let newPath = $state('');

  function addFolder() {
    if (!newPath.trim()) return;
    libraryStore.addLocalFolder(newPath);
    newPath = '';
  }
</script>

<div class="space-y-3">
  <div class="flex gap-2">
    <Input bind:value={newPath} placeholder="Add music folder path (e.g. D:\Lossless)..." />
    <Buttons variant="primary" onclick={addFolder}>Add Folder</Buttons>
  </div>

  <div class="space-y-1.5">
    {#each libraryStore.localFolders as folder (folder)}
      <div class="flex items-center justify-between rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5">
        <div class="flex min-w-0 items-center gap-2.5">
          <WaveIcon name="folderOpen" size={16} class="text-[var(--lw-text-secondary)]" />
          <span class="truncate text-[13px]">{folder}</span>
        </div>
        <button
          type="button"
          onclick={() => libraryStore.removeLocalFolder(folder)}
          class="cursor-pointer rounded-[8px] p-1.5 text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
        >
          <WaveIcon name="trash" size={14} />
        </button>
      </div>
    {/each}
  </div>
</div>
