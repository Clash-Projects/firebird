<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
  import Input from '$src/ui/components/Input.svelte';
</script>

<div class="space-y-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Download Folder Template</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">Tokens: &#123;artist&#125;, &#123;album&#125;, &#123;track_num&#125;, &#123;title&#125;</p>
  </div>
  <Input bind:value={settingsStore.downloadFolderTemplate} />
  <label class="flex cursor-pointer items-center justify-between text-[12.5px]">
    <span>Embed synced .lrc lyrics</span>
    <input type="checkbox" bind:checked={settingsStore.embedLyrics} class="h-4 w-4 accent-[var(--lw-accent)]" />
  </label>
  <label class="flex cursor-pointer items-center justify-between text-[12.5px]">
    <span>Embed high-resolution cover artwork</span>
    <input type="checkbox" bind:checked={settingsStore.embedArtwork} class="h-4 w-4 accent-[var(--lw-accent)]" />
  </label>
</div>
