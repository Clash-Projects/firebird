<script lang="ts">
  import PluginsSettings from '$src/ui/settings/PluginsSettings.svelte';
  import WaveEntrance from '$src/ui/theme/WaveEntrance.svelte';
</script>

<div class="w-full space-y-5 px-4 pt-[22px] pb-12 md:px-6 xl:px-7">
  <WaveEntrance index={0}>
    <header>
      <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        HYBRID MARKETPLACE
      </div>
      <h1 class="mt-0.5 text-[24px] font-bold tracking-[-0.025em]">Plugins & Themes</h1>
      <p class="mt-0.5 text-[12.5px] text-[var(--lw-text-secondary)]">
        Discover Official & Community Palettes, Themes, Snippets, and Sandboxed Widgets from Clash-Projects/lastwave-plugins.
      </p>
    </header>
  </WaveEntrance>

  <WaveEntrance index={1}>
    <PluginsSettings />
  </WaveEntrance>
</div>
