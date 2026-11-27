<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { t } from '$src/core/l10n/appLocalizations.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import { NAV_DESTINATIONS, type NavDestination } from './destinations';

  const listen = NAV_DESTINATIONS.filter((item) => item.group === 'listen');
  const collection = NAV_DESTINATIONS.filter((item) => item.group === 'collection');
  const offline = NAV_DESTINATIONS.filter((item) => item.group === 'offline');
  const system = NAV_DESTINATIONS.filter((item) => item.group === 'system');

  let activePath = $derived.by(() => {
    const path = router.path;
    if (['/downloads', '/albums', '/album', '/artists', '/artist'].some((route) => router.matches(route))) return '/library';
    if (['/liked', '/playlist', '/yt-playlist', '/spotify-playlist'].some((route) => router.matches(route))) return '/playlists';
    if (router.matches('/history')) return '/stats';
    if (router.matches('/genres') || router.matches('/genre')) return '/search';
    return path;
  });
</script>

<WaveHaze level="l1" enableBlur={false} dataLw="sidebar" class="rail {windowState.railExpanded ? 'expanded' : ''}">
  <nav class="rail-nav" aria-label="Primary navigation">
    {#snippet navItem(route: string, label: string, icon: WaveIconName, active: boolean)}
      <button
        type="button"
        data-lw="sidebar-item"
        data-active={active}
        aria-current={active ? 'page' : undefined}
        aria-label={label}
        use:waveTooltip={windowState.railExpanded ? null : label}
        onclick={() => router.navigate(route)}
        class="rail-item"
      >
        <span class="rail-icon"><WaveIcon name={icon} size={18} /></span>
        <span class="rail-label">{label}</span>
      </button>
    {/snippet}

    {#snippet destination(item: NavDestination)}
      {@render navItem(item.route, t(item.labelKey, item.fallbackLabel), item.icon, activePath === item.route || activePath.startsWith(`${item.route}/`))}
    {/snippet}

    <div class="lw-scroll flex-1 overflow-x-hidden" use:waveSmoothScroll={{ axis: 'y' }}>
      <div class="rail-group">
        {#each listen as item (item.id)}{@render destination(item)}{/each}
      </div>

      <div class="rail-sep" aria-hidden="true"></div>

      <div class="rail-group">
        {#each collection as item (item.id)}{@render destination(item)}{/each}
      </div>

      <div class="rail-sep" aria-hidden="true"></div>

      <div class="rail-group">
        {#each offline as item (item.id)}{@render destination(item)}{/each}
      </div>

      {#if uiSlots.sidebarItems.length}
        <div class="rail-sep" aria-hidden="true"></div>
        <div class="rail-group">
          {#each uiSlots.sidebarItems as slot (slot.id)}
            {@render navItem(slot.route, slot.label, (slot.icon as WaveIconName) || 'plugin', router.path === slot.route)}
          {/each}
        </div>
      {/if}
    </div>

    <div class="rail-bottom-sep" aria-hidden="true"></div>

    <div class="rail-group">
      {#each system as item (item.id)}{@render destination(item)}{/each}
    </div>
  </nav>
</WaveHaze>

<style>
  :global([data-lw="sidebar"].rail) {
    flex: 0 0 var(--lw-shell-rail-width);
    width: var(--lw-shell-rail-width);
    min-width: 0;
    overflow: hidden;
    border-right: 1px solid var(--lw-divider);
    background: var(--lw-rail-translucent);
    transition:
      flex-basis var(--lw-motion-normal) var(--ease-wave),
      width var(--lw-motion-normal) var(--ease-wave);
  }

  .rail-nav {
    display: flex;
    height: 100%;
    flex-direction: column;
    padding: 6px 6px 8px;
  }

  .rail-group {
    display: grid;
    gap: 3px;
  }

  .rail-sep {
    height: 1px;
    margin: 7px 10px;
    background: var(--lw-divider);
  }

  .rail-bottom-sep {
    height: 1px;
    margin: 6px 6px 8px;
    background: var(--lw-divider);
  }

  .rail-item {
    position: relative;
    display: flex;
    width: 100%;
    min-width: 0;
    height: 40px;
    align-items: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 10px;
    cursor: pointer;
    color: var(--lw-text-secondary);
    text-align: left;
    transition:
      background-color var(--lw-motion-fast) ease,
      border-color var(--lw-motion-fast) ease,
      color var(--lw-motion-fast) ease;
  }

  .rail-item::before {
    position: absolute;
    left: 0;
    width: 3px;
    height: 16px;
    border-radius: 6px;
    background: var(--lw-accent);
    opacity: 0;
    content: '';
    transform: translateX(-4px);
    transition:
      opacity var(--lw-motion-fast) ease,
      transform var(--lw-motion-fast) var(--ease-wave);
  }

  /* OG side_rail.dart: subtle neutral active wash (8%), transparent border, primary ink — nothing loud */
  .rail-item[data-active="true"] {
    background: color-mix(in srgb, var(--lw-text-primary) 8%, transparent);
    color: var(--lw-text-primary);
  }

  .rail-item[data-active="true"]::before {
    opacity: 1;
    transform: translateX(0);
  }

  .rail-item:focus-visible {
    outline: none;
    border-color: color-mix(in srgb, var(--lw-accent) 60%, transparent);
  }

  .rail-icon {
    display: grid;
    width: 46px;
    flex: none;
    place-items: center;
    transition: transform var(--lw-motion-fast) var(--ease-wave), color var(--lw-motion-fast) ease;
  }

  .rail-item[data-active="true"] .rail-icon {
    transform: scale(1.06);
  }

  .rail-label {
    overflow: hidden;
    padding-right: 10px;
    font-size: 13px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0;
    transition: opacity var(--lw-motion-normal) var(--ease-wave);
  }

  :global([data-lw="sidebar"].rail.expanded) .rail-label {
    opacity: 1;
  }

  .rail-item[data-active="true"] .rail-label {
    font-weight: 600;
  }

  @media (hover: hover) and (pointer: fine) {
    .rail-item:hover {
      color: var(--lw-text-primary);
    }
    .rail-item[data-active="false"]:hover {
      background: color-mix(in srgb, var(--lw-text-primary) 4%, transparent);
    }
    .rail-item[data-active="false"]:hover .rail-icon {
      transform: scale(1.03);
    }
  }
</style>
