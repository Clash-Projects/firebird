<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { t } from '$src/core/l10n/appLocalizations.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
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

<WaveHaze level="l1" enableBlur={false} dataLw="sidebar" class="rail">
  <nav class="rail-nav" aria-label="Primary navigation">
    {#snippet navItem(route: string, label: string, icon: WaveIconName, active: boolean)}
      <button
        type="button"
        data-lw="sidebar-item"
        data-active={active}
        aria-current={active ? 'page' : undefined}
        aria-label={label}
        title={windowState.railExpanded ? undefined : label}
        onclick={() => router.navigate(route)}
        class="rail-item"
      >
        <span class="rail-icon"><WaveIcon name={icon} size={18} /></span>
        {#if windowState.railExpanded}<span class="rail-label">{label}</span>{/if}
      </button>
    {/snippet}

    {#snippet destination(item: NavDestination)}
      {@render navItem(item.route, t(item.labelKey, item.fallbackLabel), item.icon, activePath === item.route || activePath.startsWith(`${item.route}/`))}
    {/snippet}

    <div class="lw-scroll flex-1 overflow-x-hidden">
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
    border-right: 1px solid var(--lw-divider);
    background: var(--lw-rail-translucent);
    transition: flex-basis 180ms cubic-bezier(0.23, 1, 0.32, 1), width 180ms cubic-bezier(0.23, 1, 0.32, 1);
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
    transition: background-color 110ms ease, border-color 110ms ease, color 110ms ease;
  }

  .rail-item::before {
    position: absolute;
    left: 0;
    width: 3px;
    height: 16px;
    border-radius: 999px;
    background: var(--lw-accent);
    opacity: 0;
    content: '';
    transform: scaleY(0.65);
    transition: opacity 110ms ease, transform 110ms cubic-bezier(0.23, 1, 0.32, 1);
  }

  .rail-item[data-active="true"] {
    border-color: color-mix(in srgb, var(--lw-accent) 42%, transparent);
    background: color-mix(in srgb, var(--lw-accent) 11%, transparent);
    color: var(--lw-text-primary);
  }

  .rail-item[data-active="true"]::before {
    opacity: 1;
    transform: scaleY(1);
  }

  .rail-item[data-active="true"] .rail-icon {
    color: var(--lw-accent);
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
    transition: transform 110ms ease, color 110ms ease;
  }

  .rail-item[data-active="true"] .rail-icon {
    transform: scale(1.04);
  }

  .rail-label {
    overflow: hidden;
    padding-right: 10px;
    font-size: 13px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rail-item[data-active="true"] .rail-label {
    font-weight: 600;
  }

  @media (hover: hover) and (pointer: fine) {
    .rail-item:hover {
      color: var(--lw-text-primary);
    }
    .rail-item[data-active="false"]:hover {
      background: var(--lw-wash-hover);
    }
  }
</style>
