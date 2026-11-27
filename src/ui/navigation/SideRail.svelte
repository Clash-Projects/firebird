<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { t } from '$src/core/l10n/appLocalizations.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import { NAV_DESTINATIONS, type NavDestination } from './destinations';

  let listen = $derived(NAV_DESTINATIONS.filter((d) => d.group === 'listen'));
  let collection = $derived(NAV_DESTINATIONS.filter((d) => d.group === 'collection'));
  let offline = $derived(NAV_DESTINATIONS.filter((d) => d.group === 'offline'));
  let system = $derived(NAV_DESTINATIONS.filter((d) => d.group === 'system'));

  function activePath(): string {
    const p = router.path;
    if (p.startsWith('/downloads')) return '/library';
    if (p.startsWith('/liked') || p.startsWith('/playlist') || p.startsWith('/yt-playlist') || p.startsWith('/spotify-playlist')) return '/playlists';
    if (p.startsWith('/history')) return '/stats';
    if (p.startsWith('/home')) return '/home';
    return p;
  }

  function isActive(route: string): boolean {
    const cur = activePath();
    return cur === route || cur.startsWith(`${route}/`);
  }

  function navIcon(name?: string): WaveIconName {
    return (name as WaveIconName) || 'plugin';
  }
</script>

<WaveHaze
  level="l1"
  enableBlur={false}
  dataLw="sidebar"
  class="flex h-full shrink-0 flex-col border-r border-[var(--lw-divider)] bg-[var(--lw-rail-translucent)] transition-[width] duration-[180ms] ease-[var(--ease-wave)]"
>
  <aside
    class="flex h-full flex-col pt-1.5 pb-2"
    style:width="{windowState.railExpanded ? 200 : 60}px"
    aria-label="Primary navigation"
  >
    {#snippet navItem(item: NavDestination)}
      {@const active = isActive(item.route)}
      {@const label = t(item.labelKey, item.fallbackLabel)}
      <button
        type="button"
        data-lw="sidebar-item"
        data-active={active}
        onclick={() => router.navigate(item.route)}
        title={windowState.railExpanded ? undefined : label}
        class="group relative my-[1.5px] flex h-[40px] w-full cursor-pointer items-center overflow-hidden rounded-[10px] transition-colors duration-[110ms] {active
          ? 'bg-white/[0.08] text-[var(--lw-text-primary)]'
          : 'text-[var(--lw-text-secondary)] hover:bg-white/[0.04] hover:text-[var(--lw-text-primary)]'}"
      >
        <span
          class="absolute top-1/2 left-0 h-4 w-[3px] -translate-y-1/2 rounded-[6px] bg-[var(--lw-accent)] transition-all duration-[110ms]"
          style:opacity={active ? 1 : 0}
          style:transform={active ? 'translateY(-50%) translateX(0)' : 'translateY(-50%) translateX(-4px)'}
        ></span>

        <span
          class="flex h-full w-[46px] shrink-0 items-center justify-center transition-transform duration-[110ms] {active
            ? 'text-[var(--lw-accent)]'
            : ''}"
        >
          <WaveIcon name={item.icon} size={18} />
        </span>

        {#if windowState.railExpanded}
          <span class="truncate pr-2 text-[13px] {active ? 'font-semibold' : 'font-normal'}">
            {label}
          </span>
        {/if}
      </button>
    {/snippet}

    <div class="lw-scroll flex-1 px-[6px]">
      {#each listen as item (item.id)}
        {@render navItem(item)}
      {/each}

      <div class="mx-[10px] my-[7px] h-px bg-[var(--lw-divider)]"></div>

      {#each collection as item (item.id)}
        {@render navItem(item)}
      {/each}

      <div class="mx-[10px] my-[7px] h-px bg-[var(--lw-divider)]"></div>

      {#each offline as item (item.id)}
        {@render navItem(item)}
      {/each}

      {#if uiSlots.sidebarItems.length > 0}
        <div class="mx-[10px] my-[7px] h-px bg-[var(--lw-divider)]"></div>
        {#each uiSlots.sidebarItems as slot (slot.id)}
          {@const active = router.path === slot.route}
          <button
            type="button"
            data-lw="sidebar-item"
            data-active={active}
            onclick={() => router.navigate(slot.route)}
            title={windowState.railExpanded ? undefined : slot.label}
            class="group relative my-[1.5px] flex h-[40px] w-full cursor-pointer items-center overflow-hidden rounded-[10px] transition-colors duration-[110ms] {active
              ? 'bg-white/[0.08] text-[var(--lw-text-primary)]'
              : 'text-[var(--lw-text-secondary)] hover:bg-white/[0.04] hover:text-[var(--lw-text-primary)]'}"
          >
            <span
              class="absolute top-1/2 left-0 h-4 w-[3px] -translate-y-1/2 rounded-[6px] bg-[var(--lw-accent)] transition-all duration-[110ms]"
              style:opacity={active ? 1 : 0}
            ></span>
            <span class="flex h-full w-[46px] shrink-0 items-center justify-center {active ? 'text-[var(--lw-accent)]' : ''}">
              <WaveIcon name={navIcon(slot.icon)} size={18} />
            </span>
            {#if windowState.railExpanded}
              <span class="truncate pr-2 text-[13px] {active ? 'font-semibold' : 'font-normal'}">{slot.label}</span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>

    <div class="mx-[12px] my-[6px] h-px bg-[var(--lw-divider)]"></div>

    <div class="px-[6px]">
      {#each system as item (item.id)}
        {@render navItem(item)}
      {/each}
    </div>
  </aside>
</WaveHaze>
