<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import Switch from '$src/ui/components/Switch.svelte';

  const STRUCTURE_OPTIONS = [
    { value: 'artist_album', label: 'Artist / Album / Track' },
    { value: 'artist_year_album', label: 'Artist / (Year) Album / Track' },
    { value: 'album_only', label: 'Album / Track' },
    { value: 'year_album', label: '(Year) Album / Track' },
    { value: 'flat', label: 'Flat (Single Folder)' },
  ];
</script>

<div class="space-y-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center justify-between gap-3">
    <div>
      <div class="text-[13.5px] font-semibold">Download Folder Structure</div>
      <p class="text-[12px] text-[var(--lw-text-secondary)]">How offline audio files are organized on disk</p>
    </div>
    <Select
      label="Download folder structure"
      value={settingsStore.downloadFolderTemplate}
      options={STRUCTURE_OPTIONS}
      onchange={(v) => {
        settingsStore.downloadFolderTemplate = v;
        settingsStore.saveStringPref('lw_download_structure', v);
      }}
    />
  </div>
  <label class="flex cursor-pointer items-center justify-between text-[12.5px]">
    <span>Save synced .lrc lyrics alongside downloads</span>
    <Switch
      checked={settingsStore.embedLyrics}
      onchange={(checked) => {
        settingsStore.embedLyrics = checked;
        settingsStore.saveBoolPref('lw_download_lyrics', settingsStore.embedLyrics);
      }}
    />
  </label>
</div>
