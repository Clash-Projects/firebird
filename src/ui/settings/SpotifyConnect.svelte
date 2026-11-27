<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  function toggleSpotify() {
    settingsStore.spotifyConnected = !settingsStore.spotifyConnected;
    featuresStore.showToast({
      title: settingsStore.spotifyConnected ? 'Spotify connected' : 'Spotify disconnected',
      severity: settingsStore.spotifyConnected ? 'success' : 'info',
    });
  }
</script>

<div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center gap-3">
    <BrandIcon name="spotify" size={24} />
    <div>
      <div class="text-[13.5px] font-semibold">Spotify</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        {settingsStore.spotifyConnected ? `Connected as ${settingsStore.spotifyUser}` : 'Not connected (sp_dc)'}
      </div>
    </div>
  </div>
  <Buttons variant={settingsStore.spotifyConnected ? 'secondary' : 'primary'} size="sm" onclick={toggleSpotify}>
    {settingsStore.spotifyConnected ? 'Disconnect' : 'Connect'}
  </Buttons>
</div>
