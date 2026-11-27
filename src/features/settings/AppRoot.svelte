<script lang="ts">
  import { onMount } from 'svelte';
  import AuthGate from '$src/app/AuthGate.svelte';
  import { attachWindowLifecycle } from '$src/app/windowLifecycle';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import AppShell from '$src/ui/app_shell/AppShell.svelte';
  import { attachWaveHotkeys } from '$src/ui/app_shell/waveHotkeys';
  import { syncThemeDocument } from './themeController.svelte';

  $effect(() => {
    void settingsStore.themeMode;
    void settingsStore.amoled;
    void settingsStore.customThemeBase;
    void settingsStore.accentColor;
    void settingsStore.accentSource;
    void settingsStore.glassmorphism;
    void settingsStore.appFont;
    void settingsStore.uiScale;
    void playerStore.currentTrack?.accentColor;
    syncThemeDocument();
  });

  onMount(() => {
    pluginManager.init();
    let disconnectPlayer: (() => void) | undefined;
    playerStore.connectBackend().then((fn) => {
      disconnectPlayer = fn;
    });
    const detachKeys = attachWaveHotkeys();
    const detachLife = attachWindowLifecycle();
    return () => {
      disconnectPlayer?.();
      detachKeys();
      detachLife();
    };
  });
</script>

<AuthGate>
  <AppShell />
</AuthGate>
