<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';

  interface Props {
    onOpenGuide: () => void;
  }

  let { onOpenGuide }: Props = $props();

  let apiKey = $state('');
  let apiSecret = $state('');
  let pendingToken = $state<string | null>(null);
  let lastfmBusy = $state(false);
  let lastfmError = $state<string | null>(null);

  let lbToken = $state('');
  let lbServer = $state('https://api.listenbrainz.org');
  let lbOpen = $state(false);
  let lbBusy = $state(false);

  let libreUser = $state('');
  let librePass = $state('');
  let libreServer = $state('https://libre.fm/2.0/');
  let libreOpen = $state(false);
  let libreBusy = $state(false);

  async function refreshLastfmState() {
    if (!isTauriRuntime()) return;
    const state = await ipcInvoke<{ status?: string; username?: string; message?: string }>('lastfm_auth_state');
    if (!state) return;
    settingsStore.lastfmConnected = Boolean(state.username) && state.status === 'signedIn';
    settingsStore.lastfmUser = state.username || '';
    if (state.message && state.status === 'error') {
      lastfmError = state.message;
    }
  }

  onMount(() => {
    void refreshLastfmState();
  });

  async function saveLastfmKeys() {
    if (!apiKey.trim() || !apiSecret.trim() || lastfmBusy) return;
    lastfmBusy = true;
    lastfmError = null;
    try {
      await ipcInvoke('lastfm_save_custom_keys', {
        apiKey: apiKey.trim(),
        apiSecret: apiSecret.trim(),
      });
      featuresStore.showToast({ title: 'Last.fm API keys saved', severity: 'success' });
    } catch (err) {
      lastfmError = err instanceof Error ? err.message : 'Failed to save API keys';
    } finally {
      lastfmBusy = false;
    }
  }

  async function beginLastfmAuth() {
    if (lastfmBusy) return;
    lastfmBusy = true;
    lastfmError = null;
    try {
      if (apiKey.trim() && apiSecret.trim()) {
        await ipcInvoke('lastfm_save_custom_keys', {
          apiKey: apiKey.trim(),
          apiSecret: apiSecret.trim(),
        });
      }
      const res = await ipcInvoke<{ token: string; url: string }>('lastfm_begin_web_auth');
      if (res?.token && res?.url) {
        pendingToken = res.token;
        window.open(res.url, '_blank');
        featuresStore.showToast({
          title: 'Approve LastWave in your browser',
          subtitle: 'Then click "Complete Sign-In" below',
          severity: 'info',
        });
      } else {
        lastfmError = 'Save your Last.fm API Key and Shared Secret first (BYOK).';
      }
    } finally {
      lastfmBusy = false;
    }
  }

  async function completeLastfmAuth() {
    if (!pendingToken || lastfmBusy) return;
    lastfmBusy = true;
    lastfmError = null;
    try {
      await ipcInvoke('lastfm_complete_web_auth', { token: pendingToken });
      pendingToken = null;
      await refreshLastfmState();
      if (settingsStore.lastfmConnected) {
        featuresStore.showToast({
          title: 'Last.fm connected',
          subtitle: settingsStore.lastfmUser,
          severity: 'success',
        });
      }
    } finally {
      lastfmBusy = false;
    }
  }

  async function signOutLastfm() {
    await ipcInvoke('lastfm_sign_out');
    pendingToken = null;
    settingsStore.lastfmConnected = false;
    settingsStore.lastfmUser = '';
    featuresStore.showToast({ title: 'Signed out of Last.fm', severity: 'info' });
  }

  async function connectListenBrainz() {
    if (!lbToken.trim() || lbBusy) return;
    lbBusy = true;
    try {
      const err = await ipcInvoke<string | null>('extra_scrobblers_connect_listenbrainz', {
        token: lbToken.trim(),
        server: lbServer.trim() || 'https://api.listenbrainz.org',
      });
      if (err) {
        featuresStore.showToast({ title: 'ListenBrainz failed', subtitle: err, severity: 'error' });
      } else {
        settingsStore.listenBrainzConnected = true;
        lbToken = '';
        lbOpen = false;
        featuresStore.showToast({ title: 'ListenBrainz connected', severity: 'success' });
      }
    } finally {
      lbBusy = false;
    }
  }

  async function disconnectListenBrainz() {
    await ipcInvoke('extra_scrobblers_disconnect_listenbrainz');
    settingsStore.listenBrainzConnected = false;
    featuresStore.showToast({ title: 'ListenBrainz disconnected', severity: 'info' });
  }

  async function connectLibreFm() {
    if (!libreUser.trim() || !librePass.trim() || libreBusy) return;
    libreBusy = true;
    try {
      const err = await ipcInvoke<string | null>('extra_scrobblers_connect_librefm', {
        username: libreUser.trim(),
        password: librePass,
        server: libreServer.trim() || 'https://libre.fm/2.0/',
      });
      if (err) {
        featuresStore.showToast({ title: 'Libre.fm failed', subtitle: err, severity: 'error' });
      } else {
        settingsStore.libreFmConnected = true;
        librePass = '';
        libreOpen = false;
        featuresStore.showToast({ title: 'Libre.fm connected', severity: 'success' });
      }
    } finally {
      libreBusy = false;
    }
  }

  async function disconnectLibreFm() {
    await ipcInvoke('extra_scrobblers_disconnect_librefm');
    settingsStore.libreFmConnected = false;
    featuresStore.showToast({ title: 'Libre.fm disconnected', severity: 'info' });
  }
</script>

<div class="space-y-4">
  <div class="space-y-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div>
        <div class="text-[13.5px] font-semibold">
          Last.fm (BYOK) {settingsStore.lastfmUser ? `· ${settingsStore.lastfmUser}` : ''}
        </div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.lastfmConnected
            ? 'Connected · serialized offline scrobble queue active'
            : 'Enter your own Last.fm API Key & Shared Secret, then authorize in browser'}
        </div>
      </div>
      <div class="flex items-center gap-2">
        {#if settingsStore.lastfmConnected}
          <Buttons variant="secondary" size="sm" onclick={() => void signOutLastfm()}>Sign Out</Buttons>
        {:else if pendingToken}
          <Buttons variant="primary" size="sm" disabled={lastfmBusy} onclick={() => void completeLastfmAuth()}>
            {lastfmBusy ? 'Verifying...' : 'Complete Sign-In'}
          </Buttons>
        {:else}
          <Buttons variant="primary" size="sm" disabled={lastfmBusy} onclick={() => void beginLastfmAuth()}>
            {lastfmBusy ? 'Opening...' : 'Authorize'}
          </Buttons>
        {/if}
        <Buttons variant="ghost" size="sm" onclick={onOpenGuide}>API Key Guide</Buttons>
      </div>
    </div>

    {#if !settingsStore.lastfmConnected}
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <Input bind:value={apiKey} placeholder="Last.fm API Key" />
        <Input bind:value={apiSecret} type="password" placeholder="Last.fm Shared Secret" />
      </div>
      <div class="flex items-center justify-between gap-2">
        <span class="text-[11.5px] text-[var(--lw-text-tertiary)]">
          Keys are stored securely on-device and never shared.
        </span>
        <Buttons variant="secondary" size="sm" disabled={lastfmBusy || !apiKey.trim() || !apiSecret.trim()} onclick={() => void saveLastfmKeys()}>
          Save Keys
        </Buttons>
      </div>
    {/if}

    {#if lastfmError}
      <div class="rounded-[10px] bg-[var(--lw-danger)]/15 px-3 py-2 text-[12px] text-[var(--lw-danger)]">
        {lastfmError}
      </div>
    {/if}
  </div>

  <div class="space-y-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
    <div class="flex items-center justify-between gap-3">
      <div>
        <div class="text-[13.5px] font-semibold">ListenBrainz</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.listenBrainzConnected ? 'Connected' : 'Connect using your ListenBrainz user token'}
        </div>
      </div>
      {#if settingsStore.listenBrainzConnected}
        <Buttons variant="secondary" size="sm" onclick={() => void disconnectListenBrainz()}>Disconnect</Buttons>
      {:else}
        <Buttons variant="secondary" size="sm" onclick={() => (lbOpen = !lbOpen)}>
          {lbOpen ? 'Cancel' : 'Connect'}
        </Buttons>
      {/if}
    </div>
    {#if lbOpen && !settingsStore.listenBrainzConnected}
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <Input bind:value={lbToken} type="password" placeholder="ListenBrainz User Token" />
        <Input bind:value={lbServer} placeholder="Server URL (default api.listenbrainz.org)" />
      </div>
      <div class="flex justify-end">
        <Buttons variant="primary" size="sm" disabled={lbBusy || !lbToken.trim()} onclick={() => void connectListenBrainz()}>
          {lbBusy ? 'Connecting...' : 'Save & Connect'}
        </Buttons>
      </div>
    {/if}
  </div>

  <div class="space-y-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
    <div class="flex items-center justify-between gap-3">
      <div>
        <div class="text-[13.5px] font-semibold">Libre.fm</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.libreFmConnected ? 'Connected' : 'Connect with your Libre.fm username and password'}
        </div>
      </div>
      {#if settingsStore.libreFmConnected}
        <Buttons variant="secondary" size="sm" onclick={() => void disconnectLibreFm()}>Disconnect</Buttons>
      {:else}
        <Buttons variant="secondary" size="sm" onclick={() => (libreOpen = !libreOpen)}>
          {libreOpen ? 'Cancel' : 'Connect'}
        </Buttons>
      {/if}
    </div>
    {#if libreOpen && !settingsStore.libreFmConnected}
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
        <Input bind:value={libreUser} placeholder="Username" />
        <Input bind:value={librePass} type="password" placeholder="Password" />
        <Input bind:value={libreServer} placeholder="Server URL" />
      </div>
      <div class="flex justify-end">
        <Buttons variant="primary" size="sm" disabled={libreBusy || !libreUser.trim() || !librePass.trim()} onclick={() => void connectLibreFm()}>
          {libreBusy ? 'Connecting...' : 'Save & Connect'}
        </Buttons>
      </div>
    {/if}
  </div>
</div>
