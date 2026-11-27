<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';
</script>

<div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center gap-3">
    <BrandIcon name="spotify" size={24} />
    <div>
      <div class="text-[13.5px] font-semibold">Spotify</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        {settingsStore.spotifyConnected && settingsStore.spotifyUser
          ? `Connected as ${settingsStore.spotifyUser}`
          : 'Not connected'}
      </div>
    </div>
  </div>
</div>
