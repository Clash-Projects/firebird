<script lang="ts">
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';

  let artwork = $derived(playerStore.currentTrack?.artworkUrl ?? '');
  let accent = $derived(playerStore.currentTrack?.accentColor || 'var(--lw-accent)');
  let pluginThemeActive = $derived(
    !pluginManager.safeMode && !pluginManager.allDisabled && Boolean(pluginManager.activeThemeId)
  );
  let glassActive = $derived(!pluginThemeActive && !settingsStore.performanceMode && settingsStore.glassmorphism);
</script>

{#if glassActive}
  <div data-lw="glass-stage" class="pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-[var(--lw-bg-deep)]" aria-hidden="true">
    {#if artwork}
      {#key artwork}
        <img
          src={artwork}
          alt=""
          class="absolute inset-0 h-full w-full scale-125 object-cover opacity-50 blur-[68px] saturate-125 transition-opacity duration-1000"
        />
      {/key}
      <!-- Luminance clamp + dock vignette + subtle film dither (mirrors shaders/glass_stage.frag) -->
      <div
        class="glass-scrim absolute inset-0"
      ></div>
    {:else}
      <!-- _AccentWash fallback when nothing is playing -->
      <div
        class="absolute inset-0 transition-colors duration-700"
        style:background="linear-gradient(180deg, color-mix(in srgb, {accent} 22%, var(--lw-bg)) 0%, var(--lw-bg) 100%)"
      ></div>
    {/if}
  </div>
{/if}

<style>
  .glass-scrim {
    background:
      linear-gradient(180deg, rgba(10, 10, 10, 0.42) 0%, rgba(10, 10, 10, 0.62) 60%, rgba(10, 10, 10, 0.84) 100%),
      radial-gradient(ellipse at 50% 35%, transparent 25%, rgba(10, 10, 10, 0.45) 100%);
  }

  :global(:root[data-theme="light"]) .glass-scrim {
    background:
      linear-gradient(180deg, rgba(243, 243, 243, 0.72) 0%, rgba(243, 243, 243, 0.82) 65%, rgba(243, 243, 243, 0.92) 100%),
      radial-gradient(ellipse at 50% 35%, transparent 30%, rgba(236, 236, 236, 0.45) 100%);
  }
</style>
