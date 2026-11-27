<script lang="ts">
  import { onMount } from 'svelte';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import type { MarketplacePluginEntry, PluginType } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import WaveEntrance from '$src/ui/theme/WaveEntrance.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  type StoreTab = 'marketplace' | 'installed' | 'palette' | 'theme' | 'snippet' | 'widget' | 'import';

  let tab = $state<StoreTab>('marketplace');
  let query = $state('');
  let installingId = $state<string | null>(null);

  let githubUrl = $state('');
  let installingUrl = $state(false);

  let newName = $state('');
  let newAuthor = $state('Community');
  let newType = $state<PluginType>('palette');
  let newCode = $state('');

  onMount(() => {
    if (pluginManager.marketplace.length === 0) {
      void pluginManager.fetchMarketplace();
    }
  });

  let filteredMarketplace = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return pluginManager.marketplace.filter((item) => {
      if (tab !== 'marketplace' && tab !== 'installed' && tab !== 'import' && item.type !== tab) {
        return false;
      }
      if (!q) return true;
      return (
        item.name.toLowerCase().includes(q) ||
        item.description.toLowerCase().includes(q) ||
        item.author.toLowerCase().includes(q)
      );
    });
  });

  let filteredInstalled = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return pluginManager.plugins.filter((rec) => {
      if (tab !== 'installed' && tab !== 'marketplace' && tab !== 'import' && rec.manifest.type !== tab) {
        return false;
      }
      if (!q) return true;
      return (
        rec.manifest.name.toLowerCase().includes(q) ||
        rec.manifest.description.toLowerCase().includes(q) ||
        rec.manifest.author.toLowerCase().includes(q)
      );
    });
  });

  function typeLabel(t: PluginType): string {
    if (t === 'palette') return 'Palette';
    if (t === 'theme') return 'Theme';
    if (t === 'snippet') return 'Snippet';
    return 'Widget';
  }

  function installedFor(id: string) {
    return pluginManager.plugins.find((p) => p.manifest.id === id);
  }

  async function installRegistryItem(entry: MarketplacePluginEntry) {
    if (installingId) return;
    installingId = entry.id;
    try {
      await pluginManager.installFromRegistry(entry);
    } catch (err) {
      featuresStore.showToast({
        title: `Install failed (${entry.name})`,
        subtitle: err instanceof Error ? err.message : 'Network error',
        severity: 'error',
      });
    } finally {
      installingId = null;
    }
  }

  async function installFromGitHub(e: SubmitEvent) {
    e.preventDefault();
    if (!githubUrl.trim() || installingUrl) return;
    installingUrl = true;
    try {
      await pluginManager.installFromUrl(githubUrl);
      githubUrl = '';
      tab = 'installed';
    } catch (err) {
      featuresStore.showToast({
        title: 'GitHub install failed',
        subtitle: err instanceof Error ? err.message : 'Could not load manifest.json',
        severity: 'error',
      });
    } finally {
      installingUrl = false;
    }
  }

  async function installManual(e: SubmitEvent) {
    e.preventDefault();
    if (!newName.trim() || !newCode.trim()) return;
    const id = `${newType}-${newName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${Date.now().toString(36)}`;
    const entry =
      newType === 'palette'
        ? 'colors.json'
        : newType === 'theme'
          ? 'theme.css'
          : newType === 'snippet'
            ? 'snippet.css'
            : 'widget.js';
    try {
      await pluginManager.installPlugin(
        {
          id,
          name: newName.trim(),
          version: '1.0.0',
          apiVersion: 1,
          minAppVersion: '2.6.0',
          author: newAuthor.trim() || 'Community',
          description: `Custom ${typeLabel(newType).toLowerCase()}`,
          type: newType,
          entry,
        },
        newType === 'widget' ? { sourceCode: newCode } : { cssCode: newCode }
      );
      featuresStore.showToast({
        title: `Installed ${newName.trim()}`,
        subtitle: `${typeLabel(newType)} activated`,
        severity: 'success',
      });
      newName = '';
      newCode = '';
      tab = 'installed';
    } catch (err) {
      featuresStore.showToast({
        title: 'Install failed',
        subtitle: err instanceof Error ? err.message : 'Invalid plugin',
        severity: 'error',
      });
    }
  }
</script>

<div class="w-full space-y-6 px-4 pt-[22px] pb-12 md:px-6 xl:px-7">
  <WaveEntrance index={0}>
    <header class="flex flex-wrap items-end justify-between gap-4">
      <div>
        <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          COMMUNITY MARKETPLACE
        </div>
        <h1 class="mt-0.5 text-[24px] font-bold tracking-[-0.4px]">UI Plugins</h1>
        <p class="mt-0.5 text-[12.5px] text-[var(--lw-text-secondary)]">
          Palettes, Themes, Snippets, and Sandboxed Widgets from Clash-Projects/lastwave-plugins.
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        {#if pluginManager.activePaletteId}
          <Buttons variant="ghost" size="sm" onclick={() => pluginManager.activatePalette(null)}>
            Reset Palette
          </Buttons>
        {/if}
        {#if pluginManager.activeThemeId}
          <Buttons variant="ghost" size="sm" onclick={() => pluginManager.activateTheme(null)}>
            Reset Theme
          </Buttons>
        {/if}
        <Buttons
          variant={pluginManager.allDisabled ? 'danger' : 'ghost'}
          size="sm"
          onclick={() => void pluginManager.setAllDisabled(!pluginManager.allDisabled)}
        >
          {pluginManager.allDisabled ? 'Plugins Disabled (Enable)' : 'Disable All'}
        </Buttons>
        <div class="w-[220px]">
          <Input bind:value={query} placeholder="Search store..." />
        </div>
      </div>
    </header>
  </WaveEntrance>

  <WaveEntrance index={1}>
    <div class="flex flex-wrap items-center gap-1.5 border-b border-[var(--lw-divider)] pb-3">
      <Buttons variant="chip" size="sm" active={tab === 'marketplace'} onclick={() => (tab = 'marketplace')}>Marketplace</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'installed'} onclick={() => (tab = 'installed')}>Installed</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'palette'} onclick={() => (tab = 'palette')}>Palettes</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'theme'} onclick={() => (tab = 'theme')}>Themes</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'snippet'} onclick={() => (tab = 'snippet')}>Snippets</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'widget'} onclick={() => (tab = 'widget')}>Widgets</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'import'} onclick={() => (tab = 'import')}>
        <WaveIcon name="plus" size={13} /> Import
      </Buttons>
    </div>
  </WaveEntrance>

  {#if tab === 'import'}
    <div class="max-w-[760px] space-y-5">
      <form onsubmit={installFromGitHub} class="space-y-3 rounded-[18px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-5">
        <div class="text-[15px] font-bold">Install from GitHub</div>
        <p class="text-[12px] text-[var(--lw-text-secondary)]">
          Paste <code>owner/repo</code>, <code>owner/repo/subfolder</code>, or a direct raw <code>manifest.json</code> URL.
        </p>
        <div class="flex gap-2">
          <Input bind:value={githubUrl} placeholder="Clash-Projects/lastwave-plugins/palettes/catppuccin-mocha" />
          <Buttons type="submit" variant="primary" size="sm" disabled={installingUrl || !githubUrl.trim()}>
            {installingUrl ? 'Fetching...' : 'Install'}
          </Buttons>
        </div>
      </form>

      <form onsubmit={installManual} class="space-y-3 rounded-[18px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-5">
        <div class="text-[15px] font-bold">Create / Paste Custom Plugin</div>
        <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
          <Input bind:value={newName} placeholder="Plugin name" />
          <Input bind:value={newAuthor} placeholder="Author" />
          <Select
            bind:value={newType}
            options={[
              { value: 'palette', label: 'Palette (colors.json)' },
              { value: 'theme', label: 'Theme (theme.css)' },
              { value: 'snippet', label: 'Snippet (snippet.css)' },
              { value: 'widget', label: 'Widget (widget.js)' },
            ]}
          />
        </div>
        <textarea
          bind:value={newCode}
          rows="8"
          placeholder={newType === 'widget'
            ? 'const api = window.LastWave;\n// Runs inside sandboxed iframe'
            : newType === 'palette'
              ? '{\n  "--lw-accent": "#7be3a8"\n}'
              : '[data-lw="track-row"] {\n  height: 44px;\n}'}
          class="w-full rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-bg)] p-3 font-mono text-[12px] text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] focus:outline-none"
        ></textarea>
        <div class="flex justify-end">
          <Buttons type="submit" variant="primary" size="sm">Install & Activate</Buttons>
        </div>
      </form>
    </div>
  {:else if tab === 'marketplace' || (filteredMarketplace.length > 0 && tab !== 'installed')}
    <div class="space-y-6">
      {#if filteredMarketplace.length > 0}
        <section class="space-y-3">
          <div class="flex items-center justify-between">
            <h2 class="text-[14px] font-bold">Official Registry</h2>
            <Buttons variant="ghost" size="sm" disabled={pluginManager.marketplaceLoading} onclick={() => void pluginManager.fetchMarketplace()}>
              {pluginManager.marketplaceLoading ? 'Refreshing...' : 'Refresh'}
            </Buttons>
          </div>
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {#each filteredMarketplace as entry (entry.id)}
              {@const rec = installedFor(entry.id)}
              {@const hasUpdate = rec && rec.manifest.version !== entry.version}
              <div class="flex flex-col justify-between rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
                <div>
                  {#if entry.swatch}
                    <div class="mb-3 flex h-14 overflow-hidden rounded-[12px] border border-white/10">
                      <div class="flex-[5]" style:background={entry.swatch[0]}></div>
                      <div class="flex-[3]" style:background={entry.swatch[1]}></div>
                      <div class="flex-[2]" style:background={entry.swatch[2]}></div>
                    </div>
                  {:else}
                    <div class="mb-3 flex h-11 items-center justify-between rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-raised)] px-3.5">
                      <WaveIcon name={entry.type === 'theme' ? 'palette' : entry.type === 'snippet' ? 'wand' : 'plugin'} size={16} class="text-[var(--lw-accent)]" />
                      <span class="text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">{typeLabel(entry.type)}</span>
                    </div>
                  {/if}
                  <div class="flex items-center justify-between gap-2">
                    <h3 class="truncate text-[14.5px] font-bold">{entry.name}</h3>
                    <span class="rounded-[6px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-secondary)]">{typeLabel(entry.type)}</span>
                  </div>
                  <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">{entry.description}</p>
                </div>

                <div class="mt-4 flex items-center justify-between border-t border-[var(--lw-divider)] pt-3">
                  <span class="text-[11px] text-[var(--lw-text-tertiary)]">by {entry.author} · v{entry.version}</span>
                  <div class="flex items-center gap-1.5">
                    {#if rec && !hasUpdate}
                      <Buttons variant={rec.enabled ? 'primary' : 'secondary'} size="sm" onclick={() => void pluginManager.togglePlugin(entry.id)}>
                        {rec.enabled ? 'Active' : 'Apply'}
                      </Buttons>
                      <button
                        type="button"
                        onclick={() => void pluginManager.uninstallPlugin(entry.id)}
                        title="Uninstall"
                        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
                      >
                        <WaveIcon name="trash" size={14} />
                      </button>
                    {:else}
                      <Buttons variant="primary" size="sm" disabled={installingId === entry.id} onclick={() => void installRegistryItem(entry)}>
                        {installingId === entry.id ? 'Installing...' : hasUpdate ? 'Update' : 'Install'}
                      </Buttons>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </section>
      {:else if pluginManager.marketplaceLoading}
        <div class="py-10 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading Clash-Projects/lastwave-plugins...</div>
      {:else if pluginManager.marketplaceError}
        <div class="rounded-[12px] bg-[var(--lw-danger)]/15 px-4 py-3 text-[12.5px] text-[var(--lw-danger)]">{pluginManager.marketplaceError}</div>
      {/if}

      {#if tab !== 'marketplace' && filteredInstalled.length > 0}
        <section class="space-y-3">
          <h2 class="text-[14px] font-bold">Installed</h2>
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {#each filteredInstalled as rec (rec.manifest.id)}
              <div class="flex flex-col justify-between rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
                <div>
                  <div class="flex items-center justify-between gap-2">
                    <h3 class="truncate text-[14.5px] font-bold">{rec.manifest.name}</h3>
                    <span class="rounded-[6px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-secondary)]">{typeLabel(rec.manifest.type)}</span>
                  </div>
                  <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">{rec.manifest.description}</p>
                </div>
                <div class="mt-4 flex items-center justify-between border-t border-[var(--lw-divider)] pt-3">
                  <span class="text-[11px] text-[var(--lw-text-tertiary)]">by {rec.manifest.author} · v{rec.manifest.version}</span>
                  <Buttons variant={rec.enabled ? 'primary' : 'secondary'} size="sm" onclick={() => void pluginManager.togglePlugin(rec.manifest.id)}>
                    {rec.enabled ? 'Active' : 'Apply'}
                  </Buttons>
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/if}
    </div>
  {:else}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
      {#each filteredInstalled as rec (rec.manifest.id)}
        <div class="flex flex-col justify-between rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            {#if rec.manifest.swatch}
              <div class="mb-3 flex h-14 overflow-hidden rounded-[12px] border border-white/10">
                <div class="flex-[5]" style:background={rec.manifest.swatch[0]}></div>
                <div class="flex-[3]" style:background={rec.manifest.swatch[1]}></div>
                <div class="flex-[2]" style:background={rec.manifest.swatch[2]}></div>
              </div>
            {:else}
              <div class="mb-3 flex h-11 items-center justify-between rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-raised)] px-3.5">
                <WaveIcon name={rec.manifest.type === 'theme' ? 'palette' : rec.manifest.type === 'snippet' ? 'wand' : 'plugin'} size={16} class="text-[var(--lw-accent)]" />
                <span class="text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">{typeLabel(rec.manifest.type)}</span>
              </div>
            {/if}
            <div class="flex items-center justify-between gap-2">
              <h2 class="truncate text-[14.5px] font-bold">{rec.manifest.name}</h2>
              <span class="rounded-[6px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-secondary)]">{typeLabel(rec.manifest.type)}</span>
            </div>
            <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">{rec.manifest.description}</p>
            {#if rec.error}
              <div class="mt-2 rounded-[8px] bg-[var(--lw-danger)]/15 px-2.5 py-1 text-[11.5px] text-[var(--lw-danger)]">{rec.error}</div>
            {/if}
          </div>
          <div class="mt-4 flex items-center justify-between border-t border-[var(--lw-divider)] pt-3">
            <span class="text-[11px] text-[var(--lw-text-tertiary)]">by {rec.manifest.author} · v{rec.manifest.version}</span>
            <div class="flex items-center gap-1.5">
              <Buttons variant={rec.enabled ? 'primary' : 'secondary'} size="sm" onclick={() => void pluginManager.togglePlugin(rec.manifest.id)}>
                {rec.enabled ? (rec.manifest.type === 'palette' || rec.manifest.type === 'theme' ? 'Active' : 'Enabled') : 'Apply'}
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
        </div>
      {/each}
    </div>
  {/if}
</div>
