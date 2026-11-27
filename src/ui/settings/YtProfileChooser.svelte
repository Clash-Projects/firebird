<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  function toggleYt() {
    settingsStore.ytConnected = !settingsStore.ytConnected;
    featuresStore.showToast({
      title: settingsStore.ytConnected ? 'YouTube Music connected' : 'Signed out of YouTube Music',
      severity: settingsStore.ytConnected ? 'success' : 'info',
    });
  }
</script>

<div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center gap-3">
    <BrandIcon name="youtube" size={24} />
    <div>
      <div class="text-[13.5px] font-semibold">YouTube Music</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        {settingsStore.ytConnected
          ? `${settingsStore.ytAccountName} (${settingsStore.ytAccountEmail})`
          : 'Sign in for two-way library & liked songs sync'}
      </div>
    </div>
  </div>
  <Buttons variant={settingsStore.ytConnected ? 'secondary' : 'primary'} size="sm" onclick={toggleYt}>
    {settingsStore.ytConnected ? 'Sign Out' : 'Sign In'}
  </Buttons>
</div>
