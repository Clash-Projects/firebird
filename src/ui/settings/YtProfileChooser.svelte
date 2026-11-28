<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  interface YtConnectionResult {
    connected?: boolean;
    accountName?: string;
    profileEmail?: string;
    activePageId?: string;
    photoUrl?: string;
  }

  interface YtChannelItem {
    pageId: string;
    name: string;
    handle?: string;
    photoUrl?: string;
  }

  interface YtProfileItem {
    email: string;
    activePageId?: string;
  }

  let busy = $state(false);
  let showCookieForm = $state(false);
  let cookiesInput = $state('');
  let cookieError = $state('');
  let activePageId = $state('');
  let activePhotoUrl = $state('');
  let channels = $state<YtChannelItem[]>([]);
  let loadingChannels = $state(false);
  let profiles = $state<YtProfileItem[]>([]);
  let cookieFileInput: HTMLInputElement | undefined = $state();

  async function refreshProfiles() {
    if (!isTauriRuntime()) return;
    const list = await ipcInvoke<YtProfileItem[]>('yt_list_profiles');
    if (Array.isArray(list)) {
      profiles = list;
    }
  }

  async function refreshChannels() {
    if (!isTauriRuntime() || !settingsStore.ytConnected) {
      channels = [];
      return;
    }
    loadingChannels = true;
    try {
      const list = await ipcInvoke<YtChannelItem[]>('yt_fetch_available_channels');
      channels = Array.isArray(list) ? list : [];
      if (!activePhotoUrl) {
        const match = channels.find((c) => c.pageId === activePageId);
        if (match?.photoUrl) {
          activePhotoUrl = match.photoUrl;
          settingsStore.ytAccountAvatar = match.photoUrl;
        }
      }
    } finally {
      loadingChannels = false;
    }
  }

  onMount(() => {
    void refreshProfiles();
    if (isTauriRuntime()) {
      void ipcInvoke<YtConnectionResult>('yt_get_connection').then((conn) => {
        if (conn?.connected) {
          activePageId = conn.activePageId ?? '';
          activePhotoUrl = conn.photoUrl || settingsStore.ytAccountAvatar || '';
          if (activePhotoUrl) settingsStore.ytAccountAvatar = activePhotoUrl;
          if (!activePhotoUrl || !settingsStore.ytAccountName) {
            void ipcInvoke<{ name?: string; email?: string; photoUrl?: string }>(
              'yt_fetch_account_info',
              { forceRefresh: false, pageId: activePageId }
            ).then((acc) => {
              if (acc?.name && !settingsStore.ytAccountName) settingsStore.ytAccountName = acc.name;
              if (acc?.email && !settingsStore.ytAccountEmail) settingsStore.ytAccountEmail = acc.email;
              if (acc?.photoUrl) {
                activePhotoUrl = acc.photoUrl;
                settingsStore.ytAccountAvatar = acc.photoUrl;
              }
            });
          }
          void refreshChannels();
        }
      });
    }
  });

  function applyConnected(conn: YtConnectionResult | null) {
    if (!conn?.connected) return;
    settingsStore.ytConnected = true;
    settingsStore.ytAccountName = conn.accountName || '';
    settingsStore.ytAccountEmail = conn.profileEmail || '';
    activePageId = conn.activePageId ?? '';
    activePhotoUrl = conn.photoUrl ?? '';
    settingsStore.ytAccountAvatar = activePhotoUrl;
    showCookieForm = false;
    cookiesInput = '';
    cookieError = '';
    void libraryStore.loadHomeFeed(true);
    void libraryStore.loadLocalStorage();
    void refreshProfiles();
    void refreshChannels();
    featuresStore.showToast({ title: 'YouTube Music connected', severity: 'success' });
  }

  async function handleWebSignIn() {
    if (busy) return;
    busy = true;
    cookieError = '';
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_web_sign_in', { fresh: false });
      applyConnected(conn);
    } catch (err) {
      featuresStore.showToast({
        title: 'YouTube Music sign-in failed',
        subtitle: err instanceof Error ? err.message : String(err),
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }

  async function handleCookieConnect() {
    const raw = cookiesInput.trim();
    if (!raw || busy) return;
    busy = true;
    cookieError = '';
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_connect_cookies', { cookies: raw });
      applyConnected(conn);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      cookieError = msg;
      featuresStore.showToast({
        title: 'Cookie sign-in failed',
        subtitle: msg,
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }

  async function handleCookieFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      cookiesInput = await file.text();
      cookieError = '';
    } catch {
      cookieError = 'Could not read the selected cookie file.';
    }
  }

  async function handleSwitchProfile(profile: YtProfileItem) {
    if (busy) return;
    busy = true;
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_switch_identity', {
        profileEmail: profile.email,
        pageId: profile.activePageId ?? '',
      });
      applyConnected(conn);
    } catch (err) {
      featuresStore.showToast({
        title: 'Could not switch profile',
        subtitle: err instanceof Error ? err.message : String(err),
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }

  async function handleSwitchChannel(pageId: string) {
    if (busy || pageId === activePageId) return;
    busy = true;
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_switch_identity', {
        profileEmail: settingsStore.ytAccountEmail,
        pageId,
      });
      if (conn?.connected) {
        settingsStore.ytAccountName = conn.accountName || '';
        settingsStore.ytAccountEmail = conn.profileEmail || settingsStore.ytAccountEmail;
        activePageId = conn.activePageId ?? pageId;
        activePhotoUrl =
          conn.photoUrl ||
          channels.find((c) => c.pageId === pageId)?.photoUrl ||
          '';
        settingsStore.ytAccountAvatar = activePhotoUrl;
        void libraryStore.loadHomeFeed(true);
        void libraryStore.loadLocalStorage();
        featuresStore.showToast({
          title: 'Switched YouTube channel',
          subtitle: conn.accountName || 'Primary account',
          severity: 'success',
        });
      }
    } catch (err) {
      featuresStore.showToast({
        title: 'Could not switch channel',
        subtitle: err instanceof Error ? err.message : String(err),
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }

  async function handleSignOut() {
    if (busy) return;
    busy = true;
    try {
      await ipcInvoke('yt_sign_out');
      settingsStore.ytConnected = false;
      settingsStore.ytAccountName = '';
      settingsStore.ytAccountEmail = '';
      settingsStore.ytAccountAvatar = '';
      activePageId = '';
      activePhotoUrl = '';
      channels = [];
      void refreshProfiles();
      featuresStore.showToast({ title: 'Signed out of YouTube Music', severity: 'info' });
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-3">
      {#if settingsStore.ytConnected && (activePhotoUrl || settingsStore.ytAccountAvatar)}
        <Artwork src={activePhotoUrl || settingsStore.ytAccountAvatar} alt={settingsStore.ytAccountName || 'YouTube Music'} size={28} circle />
      {:else}
        <BrandIcon name="youtube" size={24} />
      {/if}
      <div>
        <div class="text-[13.5px] font-semibold">YouTube Music</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.ytConnected
            ? settingsStore.ytAccountName
              ? `${settingsStore.ytAccountName}${settingsStore.ytAccountEmail ? ` (${settingsStore.ytAccountEmail})` : ''}`
              : settingsStore.ytAccountEmail || 'Connected'
            : 'Sign in for personalized feed and library sync'}
        </div>
      </div>
    </div>
    <div class="flex items-center gap-2">
      {#if settingsStore.ytConnected}
        <Buttons
          variant="secondary"
          size="sm"
          disabled={busy}
          onclick={() => {
            showCookieForm = !showCookieForm;
            cookieError = '';
          }}
        >
          {showCookieForm ? 'Close' : 'Update Cookies'}
        </Buttons>
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void handleSignOut()}>
          {busy ? 'Working...' : 'Sign Out'}
        </Buttons>
      {:else}
        <Buttons
          variant="secondary"
          size="sm"
          disabled={busy}
          onclick={() => {
            showCookieForm = !showCookieForm;
            cookieError = '';
          }}
        >
          {showCookieForm ? 'Cancel' : 'Use Cookies'}
        </Buttons>
        <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void handleWebSignIn()}>
          {busy ? 'Working...' : 'Sign In'}
        </Buttons>
      {/if}
    </div>
  </div>

  {#if showCookieForm}
    <div class="space-y-2.5 border-t border-[var(--lw-outline-soft)] pt-3">
      <p class="text-[12px] text-[var(--lw-text-secondary)]">
        Paste your <code class="rounded bg-[var(--lw-surface-raised)] px-1 py-0.5 text-[11px]">music.youtube.com</code>
        cookies (raw <code class="rounded bg-[var(--lw-surface-raised)] px-1 py-0.5 text-[11px]">Cookie:</code> header,
        Netscape <code class="rounded bg-[var(--lw-surface-raised)] px-1 py-0.5 text-[11px]">cookies.txt</code>, or
        Cookie-Editor JSON). Must include <code class="rounded bg-[var(--lw-surface-raised)] px-1 py-0.5 text-[11px]">SAPISID</code>
        or <code class="rounded bg-[var(--lw-surface-raised)] px-1 py-0.5 text-[11px]">__Secure-3PAPISID</code>.
      </p>
      <textarea
        bind:value={cookiesInput}
        rows={4}
        placeholder="SAPISID=...; SID=...; HSID=...; SSID=... (or paste cookies.txt / JSON)"
        class="lw-text-field w-full resize-y rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-2.5 font-mono text-[12px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] focus:outline-none"
      ></textarea>
      {#if cookieError}
        <div class="text-[12px] text-[var(--lw-danger,#ef4444)]">{cookieError}</div>
      {/if}
      <div class="flex items-center justify-between gap-2">
        <div>
          <input
            bind:this={cookieFileInput}
            type="file"
            accept=".txt,.json,text/plain,application/json"
            class="hidden"
            onchange={(e) => void handleCookieFile(e)}
          />
          <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => cookieFileInput?.click()}>
            Load File (.txt / .json)
          </Buttons>
        </div>
        <Buttons
          variant="primary"
          size="sm"
          disabled={busy || !cookiesInput.trim()}
          onclick={() => void handleCookieConnect()}
        >
          {busy ? 'Verifying...' : 'Connect with Cookies'}
        </Buttons>
      </div>
    </div>
  {/if}

  {#if settingsStore.ytConnected && (channels.length > 0 || loadingChannels)}
    <div class="space-y-2 border-t border-[var(--lw-outline-soft)] pt-3">
      <div class="flex items-center justify-between">
        <span class="text-[11.5px] font-semibold text-[var(--lw-text-secondary)]">
          Channel / Brand Account
        </span>
        {#if loadingChannels}
          <span class="text-[11px] text-[var(--lw-text-tertiary)]">Loading channels...</span>
        {/if}
      </div>
      {#if channels.length > 0}
        <div class="flex flex-wrap gap-2">
          {#if !channels.some((c) => c.pageId === '')}
            <Buttons
              variant={activePageId === '' ? 'primary' : 'secondary'}
              size="sm"
              disabled={busy}
              onclick={() => void handleSwitchChannel('')}
            >
              Primary Account
            </Buttons>
          {/if}
          {#each channels as ch (ch.pageId)}
            <Buttons
              variant={activePageId === ch.pageId ? 'primary' : 'secondary'}
              size="sm"
              disabled={busy}
              onclick={() => void handleSwitchChannel(ch.pageId)}
            >
              <Artwork src={ch.photoUrl} alt={ch.name} size={18} circle />
              <span>{ch.name}{ch.handle ? ` (${ch.handle})` : ''}</span>
            </Buttons>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if !settingsStore.ytConnected && profiles.length > 0}
    <div class="flex flex-wrap items-center gap-2 border-t border-[var(--lw-outline-soft)] pt-3">
      <span class="text-[11.5px] text-[var(--lw-text-secondary)]">Saved accounts:</span>
      {#each profiles as profile}
        <Buttons
          variant="secondary"
          size="sm"
          disabled={busy}
          onclick={() => void handleSwitchProfile(profile)}
        >
          {profile.email}
        </Buttons>
      {/each}
    </div>
  {/if}
</div>
