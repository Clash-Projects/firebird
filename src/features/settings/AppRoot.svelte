<script lang="ts">
  import { onMount } from 'svelte';
  import AuthGate from '$src/app/AuthGate.svelte';
  import { attachWindowLifecycle } from '$src/app/windowLifecycle';
  import { lyricsPlausibleDuration } from '$src/features/lyrics/lyricAdapter';
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
    photoUrl?: string;
  }

  function applyYtConnection(conn: BackendYtConnection | null) {
    if (!conn) return;
    settingsStore.ytConnected = Boolean(conn.connected);
    settingsStore.ytAccountName = conn.accountName || '';
    settingsStore.ytAccountEmail = conn.profileEmail || '';
    if (!conn.connected) {
      settingsStore.ytAccountAvatar = '';
    } else if (conn.photoUrl) {
      settingsStore.ytAccountAvatar = conn.photoUrl;
    }
  }

  $effect(() => {
    void settingsStore.themeMode;
    void settingsStore.amoled;
    void settingsStore.customThemeBase;
    void settingsStore.accentColor;
    void settingsStore.accentSource;
    void settingsStore.glassmorphism;
    void settingsStore.hazeMaterial;
    void settingsStore.hazeIntensity;
    void settingsStore.liquidGlass;
    void settingsStore.appFont;
    void settingsStore.uiScale;
    void playerStore.currentTrack?.accentColor;
    syncThemeDocument();
  });

  $effect(() => {
    document.documentElement.toggleAttribute('data-reduced-motion', settingsStore.reduceMotion);
  });

  $effect(() => {
    const current = playerStore.currentTrack;
    if (current) {
      void featuresStore.fetchLyricsForTrack(current);
    }
  });

  // Re-validate or re-fetch lyrics when live playback duration arrives after initial 0s buffering
  $effect(() => {
    const current = playerStore.currentTrack;
    const durSec = Math.round(playerStore.durationSec);
    if (!current || durSec <= 0) return;
    const fetchedDur = featuresStore.lyricsFetchedDurationSec[current.id];
    if (fetchedDur === undefined || fetchedDur > 0) return;
    const doc = featuresStore.lyricsByTrack[current.id];
    const needsRefetch =
      !doc ||
      (!doc.isInstrumental && doc.lines.length === 0) ||
      (doc.synced && !lyricsPlausibleDuration(doc.lines, durSec));
    if (needsRefetch) {
      void featuresStore.fetchLyricsForTrack(current, {
        forceRefresh: true,
        durationOverrideSec: durSec,
      });
    } else {
      featuresStore.lyricsFetchedDurationSec = {
        ...featuresStore.lyricsFetchedDurationSec,
        [current.id]: durSec,
      };
    }
  });

  onMount(() => {
    pluginManager.init();
    let disconnectPlayer: (() => void) | undefined;
    let unlistenYt: (() => void) | undefined;
    let unlistenSync: (() => void) | undefined;

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
        void libraryStore.loadLocalStorage();
      }).then((fn) => {
        unlistenYt = fn ?? undefined;
      });
      void ipcListen<boolean>('yt-library-synced', () => {
        void libraryStore.loadLocalStorage();
      }).then((fn) => {
        unlistenSync = fn ?? undefined;
      });
    }

    const detachKeys = attachWaveHotkeys();
    const detachLife = attachWindowLifecycle();
    return () => {
      disconnectPlayer?.();
      unlistenYt?.();
      unlistenSync?.();
      detachKeys();
      detachLife();
    };
  });
</script>

<AuthGate>
  <AppShell />
</AuthGate>
