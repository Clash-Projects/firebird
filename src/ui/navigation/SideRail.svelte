<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { t } from '$src/core/l10n/appLocalizations.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import { NAV_DESTINATIONS, type NavDestination } from './destinations';

  const browse = NAV_DESTINATIONS.filter((item) => item.group === 'listen');
  const collection = NAV_DESTINATIONS.filter((item) => item.group === 'collection' || item.group === 'offline');
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
  <nav class="rail-nav" class:expanded={windowState.railExpanded} aria-label="Primary navigation">
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

    <div class="lw-scroll rail-main">
      <div class="rail-group">
        {#if windowState.railExpanded}<div class="rail-heading">Browse</div>{/if}
        {#each browse as item (item.id)}{@render destination(item)}{/each}
      </div>
      <div class="rail-group">
        {#if windowState.railExpanded}<div class="rail-heading">Collection</div>{/if}
        {#each collection as item (item.id)}{@render destination(item)}{/each}
      </div>
      {#if uiSlots.sidebarItems.length}
        <div class="rail-group">
          {#each uiSlots.sidebarItems as slot (slot.id)}
            {@render navItem(slot.route, slot.label, (slot.icon as WaveIconName) || 'plugin', router.path === slot.route)}
          {/each}
        </div>
      {/if}
    </div>

    <div class="rail-group rail-system">
      {#each system as item (item.id)}{@render destination(item)}{/each}
    </div>
  </nav>
</WaveHaze>

<style>
  :global([data-lw="sidebar"].rail) {
    flex: 0 0 var(--lw-shell-rail-width);
    min-width: 0;
    border-right: 1px solid var(--lw-divider);
    background: var(--lw-rail-translucent);
  }

  .rail-nav { display: flex; height: 100%; flex-direction: column; padding: 14px 9px 10px; }
  .rail-nav.expanded { padding-inline: 13px; }
  .rail-main { flex: 1; }
  .rail-group { display: grid; gap: 3px; }
  .rail-group + .rail-group { margin-top: 18px; }
  .rail-system { border-top: 1px solid var(--lw-divider); padding-top: 10px; }

  .rail-heading {
    padding: 0 10px 5px;
    color: var(--lw-text-tertiary);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }

  .rail-item {
    position: relative;
    display: flex;
    min-width: 0;
    height: 41px;
    align-items: center;
    padding-right: 9px;
    border-radius: 9px;
    cursor: pointer;
    color: var(--lw-text-secondary);
    text-align: left;
    transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast);
  }

  .rail-item::before {
    position: absolute;
    left: 0;
    width: 2px;
    height: 17px;
    border-radius: 2px;
    background: var(--lw-accent);
    opacity: 0;
    content: '';
    transform: scaleY(0.65);
    transition: opacity var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }

  .rail-item[data-active="true"] {
    background: color-mix(in srgb, var(--lw-accent) 13%, transparent);
    color: var(--lw-text-primary);
  }

  .rail-item[data-active="true"]::before { opacity: 1; transform: scaleY(1); }
  .rail-item[data-active="true"] .rail-icon { color: var(--lw-accent); }
  .rail-item:focus-visible { outline-offset: -2px; }
  .rail-icon { display: grid; width: 43px; flex: none; place-items: center; }
  .rail-label { overflow: hidden; font-size: 13px; font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }

  @media (hover: hover) and (pointer: fine) {
    .rail-item:hover { color: var(--lw-text-primary); }
    .rail-item[data-active="false"]:hover { background: var(--lw-wash-hover); }
  }
</style>
