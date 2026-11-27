<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  interface YtConnectionResult {
    connected?: boolean;
    accountName?: string;
    profileEmail?: string;
  }

  interface YtProfileItem {
    email: string;
    activePageId?: string;
  }

  let busy = $state(false);
  let showCookieForm = $state(false);
  let cookiesInput = $state('');
  let cookieError = $state('');
  let profiles = $state<YtProfileItem[]>([]);
  let cookieFileInput: HTMLInputElement | undefined = $state();

  async function refreshProfiles() {
    if (!isTauriRuntime()) return;
    const list = await ipcInvoke<YtProfileItem[]>('yt_list_profiles');
    if (Array.isArray(list)) {
      profiles = list;
    }
  }

  onMount(() => {
    void refreshProfiles();
  });

  function applyConnected(conn: YtConnectionResult | null) {
    if (!conn?.connected) return;
    settingsStore.ytConnected = true;
    settingsStore.ytAccountName = conn.accountName || '';
    settingsStore.ytAccountEmail = conn.profileEmail || '';
    showCookieForm = false;
    cookiesInput = '';
    cookieError = '';
    void libraryStore.loadHomeFeed(true);
    void refreshProfiles();
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

  async function handleSignOut() {
    if (busy) return;
    busy = true;
    try {
      await ipcInvoke('yt_sign_out');
      settingsStore.ytConnected = false;
      settingsStore.ytAccountName = '';
      settingsStore.ytAccountEmail = '';
      void refreshProfiles();
      featuresStore.showToast({ title: 'Signed out of YouTube Music', severity: 'info' });
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-3">
      <BrandIcon name="youtube" size={24} />
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
        class="w-full resize-y rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-bg)] p-2.5 font-mono text-[12px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] focus:border-[var(--lw-accent)] focus:outline-none"
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
