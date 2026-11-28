<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Select from '$src/ui/components/Select.svelte';

  const STRUCTURE_OPTIONS = [
    { value: 'flat', label: 'Flat', hint: '01 - Track.flac' },
    { value: 'artist_album', label: 'Artist / Album', hint: 'Artist/Album/01 - Track.flac' },
    {
      value: 'artist_year_album',
      label: 'Artist / (Year) Album',
      hint: 'Artist/(2024) Album/01 - Track.flac',
    },
    { value: 'album_only', label: 'Album', hint: 'Album/01 - Track.flac' },
    { value: 'year_album', label: '(Year) Album', hint: '(2024) Album/01 - Track.flac' },
  ] as const;

  let confirmDialog = $state<HTMLDialogElement | null>(null);
  let resultDialog = $state<HTMLDialogElement | null>(null);
  let resultText = $state('');
  let busy = $state(false);

  async function runReorganize() {
    confirmDialog?.close();
    if (!isTauriRuntime() || busy) return;
    busy = true;
    try {
      const res = await ipcInvoke<[number, number] | { moved?: number; skipped?: number }>(
        'library_downloads_reorganize'
      );
      const moved = Array.isArray(res) ? res[0] : (res?.moved ?? 0);
      const skipped = Array.isArray(res) ? res[1] : (res?.skipped ?? 0);
      resultText = `Moved ${moved}. Skipped ${skipped}.`;
      resultDialog?.showModal();
    } finally {
      busy = false;
    }
  }
</script>

<div>
  <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">Folder layout</div>
  <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
    Applies to new downloads. Existing files stay where they are until you reorganize.
  </p>
  <div class="mt-2">
    <Select
      expanded
      label="Folder layout"
      value={STRUCTURE_OPTIONS.some((o) => o.value === settingsStore.downloadFolderTemplate)
        ? settingsStore.downloadFolderTemplate
        : 'flat'}
      options={STRUCTURE_OPTIONS}
      onchange={(v) => {
        settingsStore.downloadFolderTemplate = v;
        settingsStore.saveStringPref('lw_download_structure', v);
      }}
    />
  </div>
  <div class="mt-2">
    <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => confirmDialog?.showModal()}>
      Reorganize existing downloads
    </Buttons>
  </div>
</div>

<dialog
  bind:this={confirmDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => confirmDialog?.close()}
  aria-label="Reorganize downloads"
  class="w-[min(420px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Reorganize downloads</h2>
  <p class="mt-2 text-[13px] leading-relaxed text-[var(--lw-text-secondary)]">
    Move downloaded files into the selected folders? This does not run on its own.
  </p>
  <div class="mt-5 flex justify-end gap-2">
    <Buttons variant="secondary" size="sm" onclick={() => confirmDialog?.close()}>Cancel</Buttons>
    <Buttons variant="primary" size="sm" onclick={() => void runReorganize()}>Reorganize</Buttons>
  </div>
</dialog>

<dialog
  bind:this={resultDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => resultDialog?.close()}
  aria-label="Reorganize downloads result"
  class="w-[min(380px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Reorganize downloads</h2>
  <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">{resultText}</p>
  <div class="mt-5 flex justify-end">
    <Buttons variant="primary" size="sm" onclick={() => resultDialog?.close()}>OK</Buttons>
  </div>
</dialog>
