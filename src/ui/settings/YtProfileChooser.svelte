<script lang="ts">
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  let busy = $state(false);

  async function handleAction() {
    if (busy) return;
    busy = true;
    try {
      if (settingsStore.ytConnected) {
        await ipcInvoke('yt_sign_out');
        settingsStore.ytConnected = false;
        settingsStore.ytAccountName = '';
        settingsStore.ytAccountEmail = '';
        featuresStore.showToast({ title: 'Signed out of YouTube Music', severity: 'info' });
      } else {
        const conn = await ipcInvoke<{ connected?: boolean; accountName?: string; profileEmail?: string }>(
          'yt_web_sign_in',
          { fresh: false }
        );
        if (conn?.connected) {
          settingsStore.ytConnected = true;
          settingsStore.ytAccountName = conn.accountName || '';
          settingsStore.ytAccountEmail = conn.profileEmail || '';
          void libraryStore.loadHomeFeed(true);
          featuresStore.showToast({ title: 'YouTube Music connected', severity: 'success' });
        }
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center gap-3">
    <BrandIcon name="youtube" size={24} />
    <div>
      <div class="text-[13.5px] font-semibold">YouTube Music</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        {settingsStore.ytConnected && settingsStore.ytAccountName
          ? `${settingsStore.ytAccountName}${settingsStore.ytAccountEmail ? ` (${settingsStore.ytAccountEmail})` : ''}`
          : 'Sign in for personalized feed and library sync'}
      </div>
    </div>
  </div>
  <Buttons
    variant={settingsStore.ytConnected ? 'secondary' : 'primary'}
    size="sm"
    disabled={busy}
    onclick={() => void handleAction()}
  >
    {busy ? 'Working...' : settingsStore.ytConnected ? 'Sign Out' : 'Sign In'}
  </Buttons>
</div>
