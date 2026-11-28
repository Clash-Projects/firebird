<script lang="ts">
  import { onMount } from 'svelte';
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

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

  const REQUIRED_MARKERS = ['SAPISID', '__Secure-3PAPISID', 'SID'];

  let busy = $state(false);
  let cookiesInput = $state('');
  let cookieError = $state('');
  let activePageId = $state('');
  let activePhotoUrl = $state('');
  let channels = $state<YtChannelItem[]>([]);
  let profiles = $state<YtProfileItem[]>([]);
  let isStale = $state(false);
  let cookieFileInput: HTMLInputElement | undefined = $state();
  let cookieDialog = $state<HTMLDialogElement | null>(null);
  let switchDialog = $state<HTMLDialogElement | null>(null);
  let addDialog = $state<HTMLDialogElement | null>(null);

  let detectedMissing = $derived.by(() => {
    const raw = cookiesInput.trim();
    if (!raw) return [];
    const hasSapi = raw.includes('SAPISID') || raw.includes('__Secure-3PAPISID');
    const hasSid = raw.includes('SID');
    const out: string[] = [];
    if (!hasSapi) out.push('SAPISID / __Secure-3PAPISID');
    if (!hasSid) out.push('SID');
    return out;
  });

  async function refreshProfiles() {
    if (!isTauriRuntime()) return;
    const [list, stale] = await Promise.all([
      ipcInvoke<YtProfileItem[]>('yt_list_profiles').catch(() => []),
      ipcInvoke<boolean>('yt_is_auth_stale').catch(() => false),
    ]);
    if (Array.isArray(list)) profiles = list;
    isStale = Boolean(stale);
  }

  async function refreshChannels() {
    if (!isTauriRuntime() || !settingsStore.ytConnected) {
      channels = [];
      return;
    }
    const list = await ipcInvoke<YtChannelItem[]>('yt_fetch_available_channels').catch(() => []);
    channels = Array.isArray(list) ? list : [];
    if (!activePhotoUrl) {
      const match = channels.find((c) => c.pageId === activePageId);
      if (match?.photoUrl) {
        activePhotoUrl = match.photoUrl;
        settingsStore.ytAccountAvatar = match.photoUrl;
      }
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
    cookieDialog?.close();
    cookiesInput = '';
    cookieError = '';
    isStale = false;
    void libraryStore.loadHomeFeed(true);
    void libraryStore.loadLocalStorage();
    void refreshProfiles();
    void refreshChannels();
    featuresStore.showToast({ title: 'YouTube Music connected', severity: 'success' });
  }

  async function handleWebSignIn(fresh = false) {
    if (busy) return;
    addDialog?.close();
    busy = true;
    cookieError = '';
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_web_sign_in', { fresh });
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
      cookieError = err instanceof Error ? err.message : String(err);
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

  async function handleSwitchIdentity(email: string, pageId: string) {
    if (busy) return;
    busy = true;
    try {
      const conn = await ipcInvoke<YtConnectionResult>('yt_switch_identity', {
        profileEmail: email,
        pageId,
      });
      if (conn?.connected) {
        settingsStore.ytConnected = true;
        settingsStore.ytAccountName = conn.accountName || '';
        settingsStore.ytAccountEmail = conn.profileEmail || settingsStore.ytAccountEmail;
        activePageId = conn.activePageId ?? pageId;
        activePhotoUrl =
          conn.photoUrl ||
          channels.find((c) => c.pageId === pageId)?.photoUrl ||
          '';
        settingsStore.ytAccountAvatar = activePhotoUrl;
        isStale = false;
        void libraryStore.loadHomeFeed(true);
        void libraryStore.loadLocalStorage();
        void refreshProfiles();
        void refreshChannels();
        featuresStore.showToast({
          title: 'Switched YouTube channel',
          subtitle: conn.accountName || 'Primary account',
          severity: 'success',
        });
      }
      switchDialog?.close();
    } catch (err) {
      featuresStore.showToast({
        title: 'Could not switch identity',
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
    } finally {
      busy = false;
    }
  }

  let hasMultipleIdentities = $derived(profiles.length > 1 || channels.length > 1);
  let displayAvatar = $derived(activePhotoUrl || settingsStore.ytAccountAvatar);
</script>

<div>
  {#if settingsStore.ytConnected}
    <div class="flex items-center gap-3">
      {#if displayAvatar}
        <Artwork src={displayAvatar} alt={settingsStore.ytAccountName || 'YouTube Music'} label={settingsStore.ytAccountName || 'YouTube'} size={40} circle />
      {:else}
        <WaveIcon name="video" size={18} />
      {/if}
      <div class="min-w-0 flex-1">
        <div class="truncate text-[13.5px] font-semibold">
          {settingsStore.ytAccountName || settingsStore.ytAccountEmail || 'Connected'}
        </div>
        <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
          {settingsStore.ytAccountEmail ? `${settingsStore.ytAccountEmail} · Connected` : 'Connected'}
        </div>
      </div>
      <div class="flex items-center gap-2">
        {#if hasMultipleIdentities}
          <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => switchDialog?.showModal()}>
            Switch
          </Buttons>
        {/if}
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => addDialog?.showModal()}>
          Add
        </Buttons>
        <Buttons variant="secondary" size="sm" disabled={busy} onclick={() => void handleSignOut()}>
          Disconnect
        </Buttons>
      </div>
    </div>

    {#if isStale}
      <div class="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-[10px] border border-[var(--lw-warn)]/40 bg-[var(--lw-warn)]/12 px-3.5 py-2.5 text-[12px]">
        <div>
          <div class="font-semibold text-[var(--lw-text-primary)]">YouTube Music session expired</div>
          <div class="text-[var(--lw-text-secondary)]">
            YouTube rejected the saved session. Reconnect to restore library &amp; history.
          </div>
        </div>
        <Buttons
          variant="secondary"
          size="sm"
          onclick={() => {
            cookieError = '';
            cookieDialog?.showModal();
          }}
        >
          Reconnect
        </Buttons>
      </div>
    {/if}

    <div class="mt-3 flex items-center justify-between gap-3 py-1">
      <div class="min-w-0 flex-1">
        <div class="text-[13.5px] font-semibold">Sync listening history</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          Songs you play here show up in your YouTube Music history
        </div>
      </div>
      <Switch
        label="Sync listening history"
        checked={settingsStore.syncYtHistory}
        onchange={(v) => {
          settingsStore.syncYtHistory = v;
          settingsStore.saveBoolPref('lw_sync_yt_history', v);
        }}
      />
    </div>
  {:else}
    <div class="flex items-center gap-2.5">
      <WaveIcon name="video" size={18} />
      <span class="min-w-0 flex-1 text-[13.5px] font-semibold">Not connected</span>
      <Buttons
        variant="secondary"
        size="sm"
        disabled={busy}
        onclick={() => {
          cookieError = '';
          cookieDialog?.showModal();
        }}
      >
        Use cookies
      </Buttons>
      <Buttons variant="primary" size="sm" disabled={busy} onclick={() => void handleWebSignIn(false)}>
        {busy ? 'Opening…' : 'Connect'}
      </Buttons>
    </div>

    {#if profiles.length > 0}
      <div class="mt-3 flex flex-wrap items-center gap-2 border-t border-[var(--lw-divider)] pt-2.5">
        <span class="text-[12px] text-[var(--lw-text-secondary)]">Saved accounts:</span>
        {#each profiles as p (p.email)}
          <Buttons
            variant="secondary"
            size="sm"
            disabled={busy}
            onclick={() => void handleSwitchIdentity(p.email, p.activePageId ?? '')}
          >
            {p.email}
          </Buttons>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<!-- Add to YouTube Music dialog (matches _ytAddMenu in settings_page.dart) -->
<dialog
  bind:this={addDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => addDialog?.close()}
  aria-label="Add to YouTube Music"
  class="w-[min(460px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Add to YouTube Music</h2>
  <p class="mt-1.5 text-[12px] text-[var(--lw-text-secondary)]">
    Each identity keeps its own cookie jar, so adding one never signs the others out.
  </p>
  <div class="mt-4 space-y-2">
    <button
      type="button"
      onclick={() => void handleWebSignIn(true)}
      class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-3.5 py-2.5 text-left hover:bg-[var(--lw-surface-overlay)]"
    >
      <WaveIcon name="contact" size={17} />
      <div class="min-w-0 flex-1">
        <div class="text-[13.5px] font-semibold">Google account</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          Sign in to another Google account in a browser window
        </div>
      </div>
      <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
    </button>

    <button
      type="button"
      onclick={() => {
        addDialog?.close();
        cookieError = '';
        cookieDialog?.showModal();
      }}
      class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-3.5 py-2.5 text-left hover:bg-[var(--lw-surface-overlay)]"
    >
      <WaveIcon name="link" size={17} />
      <div class="min-w-0 flex-1">
        <div class="text-[13.5px] font-semibold">Cookies</div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">
          Paste a session exported from your own browser — works when the built-in window is blocked
        </div>
      </div>
      <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
    </button>
  </div>
  <div class="mt-5 flex justify-end">
    <Buttons variant="secondary" size="sm" onclick={() => addDialog?.close()}>Cancel</Buttons>
  </div>
</dialog>

<!-- Switch channel / account dialog -->
<dialog
  bind:this={switchDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => switchDialog?.close()}
  aria-label="Switch channel"
  class="w-[min(440px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Switch channel</h2>
  <div class="mt-3 space-y-1.5">
    {#each channels as ch (ch.pageId)}
      {@const isCurrent = ch.pageId === activePageId}
      <button
        type="button"
        onclick={() => void handleSwitchIdentity(settingsStore.ytAccountEmail, ch.pageId)}
        class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-3 py-2 text-left transition {isCurrent
          ? 'bg-[var(--lw-wash-pressed)] font-semibold'
          : 'hover:bg-[var(--lw-wash-hover)]'}"
      >
        <Artwork src={ch.photoUrl} label={ch.name} size={30} circle />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[13px] font-semibold">{ch.name}</div>
          {#if ch.handle}
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{ch.handle}</div>
          {/if}
        </div>
        {#if isCurrent}
          <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
        {/if}
      </button>
    {/each}
    {#each profiles.filter((p) => p.email !== settingsStore.ytAccountEmail) as p (p.email)}
      <button
        type="button"
        onclick={() => void handleSwitchIdentity(p.email, p.activePageId ?? '')}
        class="flex w-full cursor-pointer items-center gap-3 rounded-[10px] px-3 py-2 text-left hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="contact" size={20} />
        <span class="min-w-0 flex-1 truncate text-[13px] font-medium">{p.email}</span>
      </button>
    {/each}
  </div>
  <div class="mt-5 flex justify-end">
    <Buttons variant="secondary" size="sm" onclick={() => switchDialog?.close()}>Cancel</Buttons>
  </div>
</dialog>

<!-- Connect with cookies dialog (matches _ytConnectWithCookies in settings_page.dart) -->
<dialog
  bind:this={cookieDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => cookieDialog?.close()}
  aria-label="Connect with cookies"
  class="w-[min(540px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 class="text-[18px] font-semibold">Connect with cookies</h2>
  <p class="mt-2 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
    Use this when the built-in sign-in window is blocked or cannot load. The jar comes from your own browser, so Google sees a normal desktop session.
  </p>

  <input
    bind:this={cookieFileInput}
    type="file"
    accept=".txt,.json,text/plain,application/json"
    class="hidden"
    onchange={(e) => void handleCookieFile(e)}
  />

  <div class="mt-3 flex flex-wrap gap-2">
    <Buttons
      variant="secondary"
      size="sm"
      onclick={() => window.open('https://music.youtube.com/', '_blank', 'noopener,noreferrer')}
    >
      <WaveIcon name="external" size={14} />
      Open music.youtube.com
    </Buttons>
    <Buttons variant="secondary" size="sm" onclick={() => cookieFileInput?.click()}>
      <WaveIcon name="folderOpen" size={14} />
      Import cookies.txt
    </Buttons>
  </div>

  <p class="mt-2.5 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
    In Chrome/Edge: sign in to music.youtube.com, press F12 → Network, reload the page, open any music.youtube.com request and copy its Cookie request header value. Firefox and cookies.txt exports also work.
  </p>

  <textarea
    bind:value={cookiesInput}
    rows={5}
    placeholder="SID=...; __Secure-3PAPISID=...; SAPISID=..."
    class="lw-text-field mt-3 w-full resize-y rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3 font-mono text-[12px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)]"
  ></textarea>

  {#if cookiesInput.trim()}
    <div class="mt-2 flex items-start gap-1.5 text-[12px] text-[var(--lw-text-secondary)]">
      <WaveIcon
        name={detectedMissing.length === 0 ? 'check' : 'alert'}
        size={14}
        class="mt-0.5 shrink-0 {detectedMissing.length === 0 ? 'text-[var(--lw-success)]' : 'text-[var(--lw-warn)]'}"
      />
      <span>
        {detectedMissing.length === 0
          ? 'Session markers detected — this jar looks complete.'
          : `Not detected yet: ${detectedMissing.join(' · ')}. YouTube needs these (${REQUIRED_MARKERS.join(', ')}) to treat the jar as signed in.`}
      </span>
    </div>
  {/if}

  {#if cookieError}
    <p class="mt-2 text-[12px] text-[var(--lw-danger)]">{cookieError}</p>
  {/if}

  <div class="mt-5 flex justify-end gap-2">
    <Buttons variant="secondary" size="sm" onclick={() => cookieDialog?.close()}>Cancel</Buttons>
    <Buttons
      variant="primary"
      size="sm"
      disabled={busy || !cookiesInput.trim()}
      onclick={() => void handleCookieConnect()}
    >
      {busy ? 'Connecting…' : 'Connect'}
    </Buttons>
  </div>
</dialog>
