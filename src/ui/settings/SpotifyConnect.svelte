<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  let spDcInput = $state('');
  let showTokenInput = $state(false);
  let busy = $state(false);

  onMount(async () => {
    if (!isTauriRuntime()) return;
    const connected = await ipcInvoke<boolean>('spotify_connected');
    if (typeof connected === 'boolean') settingsStore.spotifyConnected = connected;
  });

  async function connect() {
    const token = spDcInput.trim();
    if (!token || busy) return;
    busy = true;
    try {
      const status = await ipcInvoke<string>('spotify_connect_sp_dc', { raw: token });
      if (status === 'ok') {
        settingsStore.spotifyConnected = true;
        showTokenInput = false;
        spDcInput = '';
        featuresStore.showToast({ title: 'Spotify connected', severity: 'success' });
      } else {
        featuresStore.showToast({
          title: 'Spotify sign-in failed',
          subtitle: status === 'refused' ? 'Cookie rejected or expired' : 'Could not verify sp_dc cookie',
          severity: 'error',
        });
      }
    } finally {
      busy = false;
    }
  }

  async function disconnect() {
    if (busy) return;
    busy = true;
    try {
      await ipcInvoke('spotify_disconnect');
      settingsStore.spotifyConnected = false;
      settingsStore.spotifyUser = '';
      featuresStore.showToast({ title: 'Spotify disconnected', severity: 'info' });
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-3">
      <BrandIcon name="spotify" size={24} />
      <div>
        <div class="text-[13.5px] font-semibold">Spotify</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.spotifyConnected
            ? settingsStore.spotifyUser
              ? `Connected as ${settingsStore.spotifyUser}`
              : 'Connected'
            : 'Not connected (sp_dc)'}
        </div>
      </div>
    </div>
    {#if settingsStore.spotifyConnected}
      <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void disconnect()}>Disconnect</Buttons>
    {:else}
      <Buttons variant="primary" size="sm" onclick={() => (showTokenInput = !showTokenInput)}>
        {showTokenInput ? 'Cancel' : 'Connect'}
      </Buttons>
    {/if}
  </div>

  {#if showTokenInput && !settingsStore.spotifyConnected}
    <div class="flex gap-2">
      <Input bind:value={spDcInput} placeholder="Paste Spotify sp_dc cookie..." />
      <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void connect()}>
        {busy ? 'Saving...' : 'Save'}
      </Buttons>
    </div>
  {/if}
</div>
