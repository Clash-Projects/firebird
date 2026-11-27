<script lang="ts">
  import { onMount } from 'svelte';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import type { MarketplacePluginEntry, PluginType } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  type FilterTab = 'all' | 'palette' | 'theme' | 'snippet' | 'widget';

  let view = $state<'marketplace' | 'installed'>('marketplace');
  let category = $state<FilterTab>('all');
  let search = $state('');
  let githubUrl = $state('');
  let installingId = $state<string | null>(null);
  let installingUrl = $state(false);

  onMount(() => {
    if (pluginManager.marketplace.length === 0) {
      void pluginManager.fetchMarketplace();
    }
  });

  let filteredMarketplace = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return pluginManager.marketplace.filter((item) => {
      if (category !== 'all' && item.type !== category) return false;
      if (!q) return true;
      return (
        item.name.toLowerCase().includes(q) ||
        item.description.toLowerCase().includes(q) ||
        item.author.toLowerCase().includes(q)
      );
    });
  });

  let filteredInstalled = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return pluginManager.plugins.filter((rec) => {
      if (category !== 'all' && rec.manifest.type !== category) return false;
      if (!q) return true;
      return (
        rec.manifest.name.toLowerCase().includes(q) ||
        rec.manifest.description.toLowerCase().includes(q) ||
        rec.manifest.author.toLowerCase().includes(q)
      );
    });
  });

  let activeCount = $derived(
    pluginManager.safeMode || pluginManager.allDisabled
      ? 0
      : pluginManager.plugins.filter((p) => p.enabled).length
  );

  function installedRecordFor(id: string) {
    return pluginManager.plugins.find((p) => p.manifest.id === id);
  }

  function typeLabel(t: PluginType): string {
    if (t === 'palette') return 'Palette';
    if (t === 'theme') return 'Theme';
    if (t === 'snippet') return 'Snippet';
    return 'Widget';
  }

  async function installEntry(entry: MarketplacePluginEntry) {
    if (installingId) return;
    installingId = entry.id;
    try {
      await pluginManager.installFromRegistry(entry);
    } catch (err) {
      featuresStore.showToast({
        title: `Failed to install ${entry.name}`,
        subtitle: err instanceof Error ? err.message : 'Network or manifest error',
        severity: 'error',
      });
    } finally {
      installingId = null;
    }
  }

  async function handleUrlInstall(e: SubmitEvent) {
    e.preventDefault();
    if (!githubUrl.trim() || installingUrl) return;
    installingUrl = true;
    try {
      await pluginManager.installFromUrl(githubUrl);
      githubUrl = '';
    } catch (err) {
      featuresStore.showToast({
        title: 'GitHub install failed',
        subtitle: err instanceof Error ? err.message : 'Could not fetch manifest.json',
        severity: 'error',
      });
    } finally {
      installingUrl = false;
    }
  }
</script>

<div class="space-y-4">
  {#if pluginManager.safeMode}
    <div class="rounded-[12px] border border-[var(--lw-warn)]/40 bg-[var(--lw-warn)]/12 px-4 py-3 text-[12.5px] text-[var(--lw-warn)]">
      Started in <strong>--safe-mode</strong>. All plugins are temporarily suspended.
    </div>
  {/if}

  <div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
    <div>
      <div class="text-[14px] font-semibold">Plugin Engine ({activeCount} active)</div>
      <p class="text-[12px] text-[var(--lw-text-secondary)]">
        Palettes, Themes, Snippets & Sandboxed Widgets · stored in app data
      </p>
    </div>
    <label class="flex cursor-pointer items-center gap-2 text-[12.5px] font-semibold text-[var(--lw-danger)]">
      <input
        type="checkbox"
        checked={pluginManager.allDisabled}
        onchange={(e) => void pluginManager.setAllDisabled(e.currentTarget.checked)}
        class="h-4 w-4 accent-[var(--lw-danger)]"
      />
      Disable All Plugins
    </label>
  </div>

  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="flex items-center gap-1.5">
      <Buttons variant="chip" size="sm" active={view === 'marketplace'} onclick={() => (view = 'marketplace')}>
        Browse Community
      </Buttons>
      <Buttons variant="chip" size="sm" active={view === 'installed'} onclick={() => (view = 'installed')}>
        Installed ({pluginManager.plugins.length})
      </Buttons>
    </div>
    <div class="flex items-center gap-2">
      <div class="w-[200px]">
        <Input bind:value={search} placeholder="Search plugins..." />
      </div>
      {#if view === 'marketplace'}
        <Buttons variant="ghost" size="sm" disabled={pluginManager.marketplaceLoading} onclick={() => void pluginManager.fetchMarketplace()}>
          {pluginManager.marketplaceLoading ? 'Loading...' : 'Refresh'}
        </Buttons>
      {/if}
    </div>
  </div>

  <div class="flex flex-wrap gap-1.5">
    {#each (['all', 'palette', 'theme', 'snippet', 'widget'] as const) as tab (tab)}
      <Buttons variant="chip" size="sm" active={category === tab} onclick={() => (category = tab)}>
        {tab === 'all' ? 'All' : `${typeLabel(tab)}s`}
      </Buttons>
    {/each}
  </div>

  <form onsubmit={handleUrlInstall} class="flex gap-2">
    <Input bind:value={githubUrl} placeholder="Install from GitHub (owner/repo or raw manifest.json URL)..." />
    <Buttons type="submit" variant="secondary" size="sm" disabled={installingUrl || !githubUrl.trim()}>
      {installingUrl ? 'Installing...' : 'Install URL'}
    </Buttons>
  </form>

  {#if view === 'marketplace'}
    {#if pluginManager.marketplaceError}
      <div class="rounded-[12px] bg-[var(--lw-danger)]/15 px-3.5 py-2.5 text-[12px] text-[var(--lw-danger)]">
        {pluginManager.marketplaceError}
      </div>
    {/if}

    {#if pluginManager.marketplaceLoading && pluginManager.marketplace.length === 0}
      <div class="py-8 text-center text-[13px] text-[var(--lw-text-secondary)]">
        Loading Clash-Projects/lastwave-plugins...
      </div>
    {:else}
      <div class="space-y-2.5">
        {#each filteredMarketplace as entry (entry.id)}
          {@const installed = installedRecordFor(entry.id)}
          {@const hasUpdate = installed && installed.manifest.version !== entry.version}
          <div class="flex items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3.5">
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="truncate text-[13.5px] font-semibold">{entry.name}</span>
                <span class="rounded-[5px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-secondary)] uppercase">
                  {typeLabel(entry.type)}
                </span>
                <span class="text-[11px] text-[var(--lw-text-tertiary)]">v{entry.version} · {entry.author}</span>
              </div>
              <p class="mt-0.5 truncate text-[12px] text-[var(--lw-text-secondary)]">{entry.description}</p>
            </div>
            <div class="flex shrink-0 items-center gap-1.5">
              {#if installed && !hasUpdate}
                <Buttons
                  variant={installed.enabled ? 'primary' : 'secondary'}
                  size="sm"
                  onclick={() => void pluginManager.togglePlugin(entry.id)}
                >
                  {installed.enabled ? 'Active' : 'Enable'}
                </Buttons>
                {#if !installed.builtin}
                  <button
                    type="button"
                    onclick={() => void pluginManager.uninstallPlugin(entry.id)}
                    title="Uninstall"
                    class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
                  >
                    <WaveIcon name="trash" size={14} />
                  </button>
                {/if}
              {:else}
                <Buttons
                  variant="primary"
                  size="sm"
                  disabled={installingId === entry.id}
                  onclick={() => void installEntry(entry)}
                >
                  {installingId === entry.id ? 'Installing...' : hasUpdate ? 'Update' : 'Install'}
                </Buttons>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="space-y-2">
      {#each filteredInstalled as rec (rec.manifest.id)}
        <div class="flex items-center justify-between gap-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-[13px] font-semibold">{rec.manifest.name}</span>
              <span class="rounded-[5px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] text-[var(--lw-text-secondary)] uppercase">
                {typeLabel(rec.manifest.type)}
              </span>
              <span class="text-[11px] text-[var(--lw-text-tertiary)]">v{rec.manifest.version}</span>
            </div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{rec.manifest.description}</div>
            {#if rec.error}
              <div class="mt-1 text-[11px] text-[var(--lw-danger)]">{rec.error}</div>
            {/if}
          </div>
          <div class="flex items-center gap-1.5">
            <Buttons
              variant={rec.enabled ? 'primary' : 'secondary'}
              size="sm"
              disabled={pluginManager.safeMode || pluginManager.allDisabled}
              onclick={() => void pluginManager.togglePlugin(rec.manifest.id)}
            >
              {rec.enabled ? 'On' : 'Off'}
            </Buttons>
            {#if !rec.builtin}
              <button
                type="button"
                onclick={() => void pluginManager.uninstallPlugin(rec.manifest.id)}
                title="Uninstall"
                class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
              >
                <WaveIcon name="trash" size={14} />
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
