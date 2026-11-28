<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    onOpenGuide: () => void;
  }

  let { onOpenGuide }: Props = $props();

  let apiKey = $state('');
  let apiSecret = $state('');
  let editingLfmKeys = $state(false);
  let handshake = $state<{ token: string; url: string } | null>(null);
  let lfmError = $state<string | null>(null);
  let lbError = $state<string | null>(null);
  let lfError = $state<string | null>(null);
  let busy = $state(false);

  let lbToken = $state('');
  let lbServer = $state('');
  let libreUser = $state('');
  let librePass = $state('');
  let libreServer = $state('');

  let connectedServices = $derived.by(() => {
    const out: Array<{ id: string; label: string }> = [];
    if (settingsStore.lastfmConnected) out.push({ id: 'lastfm', label: 'Last.fm' });
    if (settingsStore.listenBrainzConnected) out.push({ id: 'listenbrainz', label: 'ListenBrainz' });
    if (settingsStore.libreFmConnected) out.push({ id: 'librefm', label: 'Libre.fm' });
    return out;
  });

  async function refreshLastfmState() {
    if (!isTauriRuntime()) return;
    const state = await ipcInvoke<{ status?: string; username?: string; message?: string }>(
      'lastfm_auth_state'
    );
    if (!state) return;
    settingsStore.lastfmConnected = Boolean(state.username) && state.status === 'signedIn';
    settingsStore.lastfmUser = state.username || '';
    if (state.message && state.status === 'error') {
      lfmError = state.message;
    }
  }

  onMount(() => {
    void refreshLastfmState();
  });

  async function saveLastfmKeys() {
    if (!apiKey.trim() || !apiSecret.trim() || busy) return;
    busy = true;
    lfmError = null;
    try {
      await ipcInvoke('lastfm_save_custom_keys', {
        apiKey: apiKey.trim(),
        apiSecret: apiSecret.trim(),
      });
      settingsStore.lastfmConfigured = true;
      editingLfmKeys = false;
    } catch (err) {
      lfmError = err instanceof Error ? err.message : 'Failed to save API keys.';
    } finally {
      busy = false;
    }
  }

  async function clearLastfmKeys() {
    if (busy) return;
    busy = true;
    try {
      await ipcInvoke('lastfm_clear_custom_keys');
      settingsStore.lastfmConfigured = false;
      apiKey = '';
      apiSecret = '';
      handshake = null;
    } finally {
      busy = false;
    }
  }

  async function beginLastfmAuth() {
    if (busy) return;
    busy = true;
    lfmError = null;
    try {
      const res = await ipcInvoke<{ token: string; url: string }>('lastfm_begin_web_auth');
      if (res?.token && res?.url) {
        handshake = res;
        window.open(res.url, '_blank', 'noopener,noreferrer');
      } else {
        lfmError = 'Could not start Last.fm handshake.';
      }
    } catch {
      lfmError = 'Could not reach Last.fm. Check your connection and retry.';
    } finally {
      busy = false;
    }
  }

  async function finishLastfmAuth() {
    if (!handshake || busy) return;
    busy = true;
    lfmError = null;
    try {
      await ipcInvoke('lastfm_complete_web_auth', { token: handshake.token });
      await refreshLastfmState();
      if (settingsStore.lastfmConnected) {
        handshake = null;
      } else {
        lfmError = 'Not approved yet — approve LastWave in the browser, then retry.';
      }
    } catch {
      lfmError = 'Not approved yet — approve LastWave in the browser, then retry.';
    } finally {
      busy = false;
    }
  }

  async function signOutLastfm() {
    busy = true;
    try {
      await ipcInvoke('lastfm_sign_out');
      handshake = null;
      settingsStore.lastfmConnected = false;
      settingsStore.lastfmUser = '';
    } finally {
      busy = false;
    }
  }

  async function connectListenBrainz() {
    if (!lbToken.trim() || busy) return;
    busy = true;
    lbError = null;
    try {
      const err = await ipcInvoke<string | null>('extra_scrobblers_connect_listenbrainz', {
        token: lbToken.trim(),
        server: lbServer.trim(),
      });
      if (err) {
        lbError = err;
      } else {
        await settingsStore.loadFromBackend();
        lbToken = '';
      }
    } finally {
      busy = false;
    }
  }

  async function disconnectListenBrainz() {
    busy = true;
    try {
      await ipcInvoke('extra_scrobblers_disconnect_listenbrainz');
      settingsStore.listenBrainzConnected = false;
      settingsStore.listenBrainzUser = '';
    } finally {
      busy = false;
    }
  }

  async function connectLibreFm() {
    if (!libreUser.trim() || !librePass || busy) return;
    busy = true;
    lfError = null;
    try {
      const err = await ipcInvoke<string | null>('extra_scrobblers_connect_librefm', {
        username: libreUser.trim(),
        password: librePass,
        server: libreServer.trim(),
      });
      if (err) {
        lfError = err;
      } else {
        await settingsStore.loadFromBackend();
        librePass = '';
      }
    } finally {
      busy = false;
    }
  }

  async function disconnectLibreFm() {
    busy = true;
    try {
      await ipcInvoke('extra_scrobblers_disconnect_librefm');
      settingsStore.libreFmConnected = false;
      settingsStore.libreFmUser = '';
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-4">
  {#if connectedServices.length > 1}
    <div class="flex items-center justify-between gap-3 pb-1">
      <div class="min-w-0 flex-1">
        <div class="text-[13.5px] font-semibold">Primary scrobbler</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          Used for Stats, History, Home, and Top Artists/Albums.
        </div>
      </div>
      <div class="w-[220px] shrink-0">
        <Select
          expanded
          label="Primary scrobbler"
          value={connectedServices.some((s) => s.id === settingsStore.primaryScrobbler)
            ? settingsStore.primaryScrobbler
            : ''}
          options={[
            { value: '', label: `Auto (${connectedServices[0]?.label ?? 'Last.fm'})` },
            ...connectedServices.map((s) => ({ value: s.id, label: s.label })),
          ]}
          onchange={(v) => {
            settingsStore.primaryScrobbler = v;
            settingsStore.saveStringPref('lw_primary_scrobbler', v);
          }}
        />
      </div>
    </div>
  {/if}

  <!-- Last.fm -->
  <div>
    <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">Last.fm</div>
    {#if settingsStore.lastfmConnected}
      <div class="mt-1.5 flex items-center gap-2">
        <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
        <span class="min-w-0 flex-1 truncate text-[13.5px] font-semibold">
          Last.fm connected as @{settingsStore.lastfmUser}
        </span>
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void signOutLastfm()}>
          Disconnect
        </Buttons>
      </div>
    {:else}
      <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
        Connect with your Last.fm account and personal API keys from last.fm/api/account/create.
      </p>
      {#if !settingsStore.lastfmConfigured || editingLfmKeys}
        <div class="mt-2 flex flex-wrap items-center gap-2">
          <div class="min-w-[140px] flex-1">
            <Input bind:value={apiKey} placeholder="API key" />
          </div>
          <div class="min-w-[140px] flex-1">
            <Input bind:value={apiSecret} type="password" placeholder="Shared secret" />
          </div>
          <Buttons
            variant="primary"
            size="sm"
            disabled={busy || !apiKey.trim() || !apiSecret.trim()}
            onclick={() => void saveLastfmKeys()}
          >
            Save keys
          </Buttons>
          {#if editingLfmKeys}
            <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => (editingLfmKeys = false)}>
              Cancel
            </Buttons>
          {/if}
        </div>
        <div class="mt-1.5">
          <button
            type="button"
            onclick={onOpenGuide}
            class="cursor-pointer text-[12px] font-medium text-[var(--lw-accent)] hover:underline"
          >
            How to get personal Last.fm API keys…
          </button>
        </div>
      {:else}
        <div class="mt-2 flex flex-wrap items-center gap-2">
          <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
          <span class="min-w-0 flex-1 text-[12px] text-[var(--lw-text-secondary)]">
            {handshake === null
              ? 'API keys saved — click Connect to authorize in browser.'
              : 'Approve LastWave in your browser, then click Finish.'}
          </span>
          <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => (editingLfmKeys = true)}>
            Edit keys
          </Buttons>
          <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void clearLastfmKeys()}>
            Clear
          </Buttons>
          {#if handshake !== null}
            <Buttons
              variant="secondary"
              size="sm"
              disabled={busy}
              onclick={() => {
                if (handshake) {
                  void navigator.clipboard.writeText(handshake.url);
                  featuresStore.showToast({ title: 'Auth link copied', severity: 'success' });
                }
              }}
            >
              Copy link
            </Buttons>
            <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void beginLastfmAuth()}>
              Restart
            </Buttons>
            <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void finishLastfmAuth()}>
              Finish
            </Buttons>
          {:else}
            <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void beginLastfmAuth()}>
              Connect
            </Buttons>
          {/if}
        </div>
      {/if}
      {#if lfmError}
        <p class="mt-1.5 text-[12px] text-[var(--lw-danger)]">{lfmError}</p>
      {/if}
    {/if}
  </div>

  <!-- ListenBrainz -->
  <div>
    <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">ListenBrainz</div>
    {#if settingsStore.listenBrainzConnected}
      <div class="mt-1.5 flex items-center gap-2">
        <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
        <span class="min-w-0 flex-1 truncate text-[13.5px] font-semibold">
          ListenBrainz connected{settingsStore.listenBrainzUser ? ` as @${settingsStore.listenBrainzUser}` : ''}{settingsStore.listenBrainzServer ? ` · ${settingsStore.listenBrainzServer}` : ''}
        </span>
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void disconnectListenBrainz()}>
          Disconnect
        </Buttons>
      </div>
    {:else}
      <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
        Paste your user token from listenbrainz.org/settings. Leave the server empty unless you self-host (Maloja, Koito, …).
      </p>
      <div class="mt-2 flex flex-wrap items-center gap-2">
        <div class="min-w-[160px] flex-[3]">
          <Input bind:value={lbToken} type="password" placeholder="User token" />
        </div>
        <div class="min-w-[140px] flex-[2]">
          <Input bind:value={lbServer} placeholder="Server (optional)" />
        </div>
        <Buttons
          variant="primary"
          size="sm"
          disabled={busy || !lbToken.trim()}
          onclick={() => void connectListenBrainz()}
        >
          Connect
        </Buttons>
      </div>
      {#if lbError}
        <p class="mt-1.5 text-[12px] text-[var(--lw-danger)]">{lbError}</p>
      {/if}
    {/if}
  </div>

  <!-- Libre.fm -->
  <div>
    <div class="text-[13.5px] font-semibold text-[var(--lw-text-primary)]">Libre.fm</div>
    {#if settingsStore.libreFmConnected}
      <div class="mt-1.5 flex items-center gap-2">
        <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
        <span class="min-w-0 flex-1 truncate text-[13.5px] font-semibold">
          Libre.fm connected{settingsStore.libreFmUser ? ` as @${settingsStore.libreFmUser}` : ''}{settingsStore.libreFmServer ? ` · ${settingsStore.libreFmServer}` : ''}
        </span>
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void disconnectLibreFm()}>
          Disconnect
        </Buttons>
      </div>
    {:else}
      <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
        Sign in with your Libre.fm account. The server field takes other GNU FM instances.
      </p>
      <div class="mt-2 flex flex-wrap items-center gap-2">
        <div class="min-w-[120px] flex-1">
          <Input bind:value={libreUser} placeholder="Username" />
        </div>
        <div class="min-w-[120px] flex-1">
          <Input bind:value={librePass} type="password" placeholder="Password" />
        </div>
        <div class="min-w-[130px] flex-1">
          <Input bind:value={libreServer} placeholder="Server (optional)" />
        </div>
        <Buttons
          variant="primary"
          size="sm"
          disabled={busy || !libreUser.trim() || !librePass}
          onclick={() => void connectLibreFm()}
        >
          Connect
        </Buttons>
      </div>
      {#if lfError}
        <p class="mt-1.5 text-[12px] text-[var(--lw-danger)]">{lfError}</p>
      {/if}
    {/if}
  </div>
</div>
