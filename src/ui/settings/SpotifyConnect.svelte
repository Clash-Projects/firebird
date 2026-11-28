<script lang="ts">
  import { onMount } from 'svelte';
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let spDcInput = $state('');
  let cookieDialog = $state<HTMLDialogElement | null>(null);
  let cookieError = $state('');
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
    cookieError = '';
    try {
      const status = await ipcInvoke<string>('spotify_connect_sp_dc', { raw: token });
      if (status === 'ok') {
        settingsStore.spotifyConnected = true;
        cookieDialog?.close();
        spDcInput = '';
        featuresStore.showToast({ title: 'Spotify connected', severity: 'success' });
      } else {
        cookieError =
          status === 'refused'
            ? 'Cookie rejected or expired. Copy a fresh sp_dc value from open.spotify.com.'
            : 'Could not verify sp_dc cookie.';
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
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-3">
  <div class="flex items-center gap-2.5">
    <WaveIcon name="music" size={18} />
    <span class="min-w-0 flex-1 truncate text-[13.5px] font-semibold">
      {settingsStore.spotifyConnected
        ? settingsStore.spotifyUser
          ? `Connected · ${settingsStore.spotifyUser}`
          : 'Connected'
        : 'Not connected'}
    </span>
    {#if settingsStore.spotifyConnected}
      <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void disconnect()}>
        Disconnect
      </Buttons>
    {:else}
      <Buttons
        variant="primary"
        size="sm"
        onclick={() => {
          cookieError = '';
          cookieDialog?.showModal();
        }}
      >
        Login with cookies
      </Buttons>
    {/if}
  </div>

  <div class="flex items-center justify-between gap-3 py-1">
    <div class="min-w-0 flex-1">
      <div class="text-[13.5px] font-semibold">Spotify Canvas</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        Use Spotify's vertical looping video in Full Bleed (needs a connected account)
      </div>
    </div>
    <Switch
      label="Spotify Canvas"
      checked={settingsStore.spotifyCanvasEnabled}
      onchange={(v) => {
        settingsStore.spotifyCanvasEnabled = v;
        settingsStore.saveBoolPref('spotify_canvas_enabled', v);
      }}
    />
  </div>

  <div class="flex items-center justify-between gap-3 py-1">
    <div class="min-w-0 flex-1">
      <div class="text-[13.5px] font-semibold">Prefer Spotify Canvas</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        Prefer Spotify Canvas over Apple Music &amp; Tidal in Full Bleed
      </div>
    </div>
    <Switch
      label="Prefer Spotify Canvas"
      disabled={!settingsStore.spotifyCanvasEnabled}
      checked={settingsStore.spotifyCanvasEnabled && settingsStore.preferSpotifyCanvas}
      onchange={(v) => {
        settingsStore.preferSpotifyCanvas = v;
        settingsStore.saveBoolPref('lw_prefer_spotify_canvas', v);
      }}
    />
  </div>

  <div class="flex items-center justify-between gap-3 pt-1">
    <div class="min-w-0 flex-1">
      <div class="text-[13.5px] font-semibold">Recommendations</div>
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        {settingsStore.recommendationSource === 'spotify'
          ? settingsStore.spotifyConnected
            ? "Autoplay and radio use Spotify's own song radio."
            : 'Spotify is not connected, so YouTube Music is used until you sign in.'
          : "Autoplay and radio use YouTube Music's related songs."}
      </div>
    </div>
    <div class="w-[220px] shrink-0">
      <Select
        expanded
        label="Recommendations"
        value={settingsStore.recommendationSource}
        options={[
          { value: 'youtube', label: 'YouTube Music' },
          { value: 'spotify', label: 'Spotify' },
        ]}
        onchange={(v) => {
          settingsStore.recommendationSource = v as 'youtube' | 'spotify';
          settingsStore.saveStringPref('lw_reco_source', v);
        }}
      />
    </div>
  </div>
</div>

<dialog
  bind:this={cookieDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => cookieDialog?.close()}
  aria-label="Connect Spotify with cookies"
  class="w-[min(460px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void connect();
    }}
    class="space-y-4"
  >
    <h2 class="text-[18px] font-semibold">Connect Spotify with cookies</h2>
    <p class="text-[12.5px] leading-relaxed text-[var(--lw-text-secondary)]">
      Sign in to <button type="button" onclick={() => window.open('https://open.spotify.com/', '_blank', 'noopener,noreferrer')} class="cursor-pointer font-medium text-[var(--lw-accent)] hover:underline">open.spotify.com</button> in your browser, open DevTools → Application → Cookies, and paste the value of <code class="rounded bg-[var(--lw-surface)] px-1.5 py-0.5 font-mono text-[11.5px]">sp_dc</code>.
    </p>
    <Input bind:value={spDcInput} autofocus placeholder="AQB..." />
    {#if cookieError}
      <p class="text-[12px] text-[var(--lw-danger)]">{cookieError}</p>
    {/if}
    <div class="flex justify-end gap-2 pt-1">
      <Buttons variant="secondary" size="sm" onclick={() => cookieDialog?.close()}>Cancel</Buttons>
      <Buttons type="submit" variant="primary" size="sm" disabled={busy || !spDcInput.trim()}>
        {busy ? 'Verifying…' : 'Connect'}
      </Buttons>
    </div>
  </form>
</dialog>
