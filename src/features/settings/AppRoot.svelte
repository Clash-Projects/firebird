<script lang="ts">
  import { onMount } from 'svelte';
  import AuthGate from '$src/app/AuthGate.svelte';
  import { attachWindowLifecycle } from '$src/app/windowLifecycle';
  import { ipcInvoke, ipcListen, isTauriRuntime } from '$src/lib/ipc';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import AppShell from '$src/ui/app_shell/AppShell.svelte';
  import { attachWaveHotkeys } from '$src/ui/app_shell/waveHotkeys';
  import { syncThemeDocument } from './themeController.svelte';

  interface BackendYtConnection {
    connected?: boolean;
    accountName?: string;
    profileEmail?: string;
  }

  function applyYtConnection(conn: BackendYtConnection | null) {
    if (!conn) return;
    settingsStore.ytConnected = Boolean(conn.connected);
    settingsStore.ytAccountName = conn.accountName || '';
    settingsStore.ytAccountEmail = conn.profileEmail || '';
  }

  $effect(() => {
    void settingsStore.themeMode;
    void settingsStore.customThemeBase;
    void settingsStore.accentColor;
    void settingsStore.accentSource;
    void settingsStore.glassmorphism;
    void settingsStore.hazeMaterial;
    void settingsStore.hazeIntensity;
    void settingsStore.liquidGlass;
    void settingsStore.performanceMode;
    void settingsStore.appFont;
    void settingsStore.uiScale;
    void playerStore.currentTrack?.accentColor;
    syncThemeDocument();
  });

  $effect(() => {
    const reduced = settingsStore.reduceMotion || settingsStore.performanceMode;
    document.documentElement.toggleAttribute('data-reduced-motion', reduced);
  });

  $effect(() => {
    const current = playerStore.currentTrack;
    if (current) {
      void featuresStore.fetchLyricsForTrack(current);
    }
  });

  onMount(() => {
    pluginManager.init();
    let disconnectPlayer: (() => void) | undefined;
    let unlistenYt: (() => void) | undefined;

    playerStore.connectBackend().then((fn) => {
      disconnectPlayer = fn;
    });

    if (isTauriRuntime()) {
      void settingsStore.loadFromBackend().then((all) => {
        featuresStore.hydrateFromPrefs(all);
        playerStore.hydrateFromPrefs(all);
      });
      void ipcInvoke<BackendYtConnection>('yt_get_connection').then((conn) => {
        applyYtConnection(conn);
        if (conn?.connected) {
          void ipcInvoke<{ name?: string; handle?: string; email?: string; photoUrl?: string }>(
            'yt_fetch_account_info',
            { forceRefresh: false }
          ).then((acc) => {
            if (acc?.name) settingsStore.ytAccountName = acc.name;
            if (acc?.email && !settingsStore.ytAccountEmail) settingsStore.ytAccountEmail = acc.email;
            if (acc?.photoUrl) settingsStore.ytAccountAvatar = acc.photoUrl;
          });
        }
        void libraryStore.loadLocalStorage();
        void libraryStore.loadHomeFeed(Boolean(conn?.connected));
      });
      void ipcInvoke<boolean>('spotify_connected').then((ok) => {
        if (typeof ok === 'boolean') settingsStore.spotifyConnected = ok;
      });
      void featuresStore.connectBackend();
      void ipcListen<BackendYtConnection>('yt-connection-changed', (conn) => {
        applyYtConnection(conn);
        void libraryStore.loadHomeFeed(Boolean(conn?.connected));
      }).then((fn) => {
        unlistenYt = fn ?? undefined;
      });
    }

    const detachKeys = attachWaveHotkeys();
    const detachLife = attachWindowLifecycle();
    return () => {
      disconnectPlayer?.();
      unlistenYt?.();
      detachKeys();
      detachLife();
    };
  });
</script>

<AuthGate>
  <AppShell />
</AuthGate>
