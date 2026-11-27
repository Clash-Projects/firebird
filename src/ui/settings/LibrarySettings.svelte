<script lang="ts">
  import LibraryFolders from '$src/ui/library/LibraryFolders.svelte';
</script>

<div class="space-y-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Local Music Folders</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">Scanned automatically for FLAC, ALAC, WAV, MP3, and Opus tags.</p>
  </div>
  <LibraryFolders />
</div>
