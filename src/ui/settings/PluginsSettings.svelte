<script lang="ts">
  import { onMount } from 'svelte';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import {
    extractAuthorGithub,
    isOfficialPlugin,
    resolvePluginAssetUrl,
  } from '$src/lib/plugins/resolver';
  import type {
    MarketplacePluginEntry,
    PluginRecord,
    PluginType,
  } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import GithubInstallModal from './GithubInstallModal.svelte';
  import PluginDetailModal, { type StoreDetailSelection } from './PluginDetailModal.svelte';

  type CategoryTab = 'discover' | 'palette' | 'theme' | 'snippet' | 'widget' | 'installed';
  type SourceFilter = 'all' | 'official' | 'external';
  type SortOrder = 'featured' | 'recent' | 'alpha';

  interface StoreCardModel {
    key: string;
    id: string;
    name: string;
    type: PluginType;
    version: string;
    description: string;
    authorGithub: string;
    official: boolean;
    source: 'official' | 'external';
    updatedAt: string;
    tags: string[];
    previews: string[];
    coverUrl: string | null;
    fallbackUrl: string | null;
    swatch?: [string, string, string];
    marketEntry?: MarketplacePluginEntry;
    installedRecord?: PluginRecord;
    hasUpdate: boolean;
  }

  let activeTab = $state<CategoryTab>('discover');
  let search = $state('');
  let sourceFilter = $state<SourceFilter>('all');
  let sortOrder = $state<SortOrder>('featured');
  let githubModalOpen = $state(false);
  let detailSelection = $state<StoreDetailSelection | null>(null);
  let busyId = $state<string | null>(null);
  let coverFallbackUsed = $state<Record<string, boolean>>({});
  let coverFailed = $state<Record<string, boolean>>({});

  onMount(() => {
    if (pluginManager.marketplace.length === 0) {
      void pluginManager.fetchMarketplace();
    }
  });

  let activeCount = $derived(
    pluginManager.safeMode || pluginManager.allDisabled
      ? 0
      : pluginManager.plugins.filter((p) => p.enabled).length
  );

  function installedFor(id: string): PluginRecord | undefined {
    return pluginManager.plugins.find((p) => p.manifest.id === id);
  }

  function typeLabel(t: PluginType): string {
    if (t === 'palette') return 'Palette';
    if (t === 'theme') return 'Theme';
    if (t === 'snippet') return 'Snippet';
    return 'Widget';
  }

  function typeIcon(t: PluginType): WaveIconName {
    if (t === 'palette') return 'palette';
    if (t === 'theme') return 'sparkles';
    if (t === 'snippet') return 'wand';
    return 'plugin';
  }

  function typeBadgeClass(t: PluginType): string {
    if (t === 'palette') return 'bg-violet-500/20 text-violet-200 border-violet-400/25';
    if (t === 'theme') return 'bg-sky-500/20 text-sky-200 border-sky-400/25';
    if (t === 'snippet') return 'bg-amber-500/20 text-amber-200 border-amber-400/25';
    return 'bg-emerald-500/20 text-emerald-200 border-emerald-400/25';
  }

  function fromMarket(entry: MarketplacePluginEntry): StoreCardModel {
    const installed = installedFor(entry.id);
    const firstPreview = entry.previews[0];
    return {
      key: `market:${entry.id}`,
      id: entry.id,
      name: entry.name,
      type: entry.type,
      version: entry.version,
      description: entry.description,
      authorGithub: entry.author.github,
      official: isOfficialPlugin(entry),
      source: entry.source,
      updatedAt: entry.updatedAt,
      tags: entry.tags,
      previews: entry.previews,
      coverUrl: firstPreview ? resolvePluginAssetUrl(entry, firstPreview, false) : null,
      fallbackUrl: firstPreview ? resolvePluginAssetUrl(entry, firstPreview, true) : null,
      swatch: entry.swatch,
      marketEntry: entry,
      installedRecord: installed,
      hasUpdate: Boolean(installed && installed.manifest.version !== entry.version),
    };
  }

  function fromInstalled(rec: PluginRecord): StoreCardModel {
    const m = rec.manifest;
    const previews = m.previews ?? [];
    const firstPreview = previews[0];
    const source: 'official' | 'external' = m.source === 'external' || m.repo ? 'external' : 'official';
    return {
      key: `installed:${m.id}`,
      id: m.id,
      name: m.name,
      type: m.type,
      version: m.version,
      description: m.description,
      authorGithub: extractAuthorGithub(m.author),
      official: !rec.builtin && isOfficialPlugin({ source: m.source, author: m.author }),
      source,
      updatedAt: m.updatedAt || '2026-11-27',
      tags: m.tags ?? [],
      previews,
      coverUrl: firstPreview ? resolvePluginAssetUrl(m, firstPreview, false) : null,
      fallbackUrl: firstPreview ? resolvePluginAssetUrl(m, firstPreview, true) : null,
      swatch: m.swatch,
      installedRecord: rec,
      hasUpdate: false,
    };
  }

  let visibleCards = $derived.by(() => {
    const base: StoreCardModel[] =
      activeTab === 'installed'
        ? pluginManager.plugins.map(fromInstalled)
        : pluginManager.marketplace.map(fromMarket);

    const q = search.trim().toLowerCase().replace(/^@/, '');
    const filtered = base.filter((card) => {
      if (
        activeTab !== 'discover' &&
        activeTab !== 'installed' &&
        card.type !== activeTab
      ) {
        return false;
      }
      if (activeTab !== 'installed' && sourceFilter !== 'all' && card.source !== sourceFilter) {
        return false;
      }
      if (!q) return true;
      return (
        card.name.toLowerCase().includes(q) ||
        card.description.toLowerCase().includes(q) ||
        card.authorGithub.toLowerCase().includes(q) ||
        card.tags.some((t) => t.toLowerCase().includes(q))
      );
    });

    if (sortOrder === 'alpha') {
      return [...filtered].sort((a, b) => a.name.localeCompare(b.name));
    }
    if (sortOrder === 'recent') {
      return [...filtered].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    }
    return filtered;
  });

  function openDetail(card: StoreCardModel) {
    if (card.marketEntry) {
      detailSelection = { kind: 'market', entry: card.marketEntry };
    } else if (card.installedRecord) {
      detailSelection = { kind: 'installed', record: card.installedRecord };
    }
  }

  async function handleCardAction(e: MouseEvent, card: StoreCardModel) {
    e.stopPropagation();
    if (busyId) return;
    busyId = card.id;
    try {
      if (card.installedRecord && !card.hasUpdate) {
        await pluginManager.togglePlugin(card.id);
      } else if (card.marketEntry) {
        await pluginManager.installFromRegistry(card.marketEntry);
      }
    } catch (err) {
      featuresStore.showToast({
        title: `Failed (${card.name})`,
        subtitle: err instanceof Error ? err.message : 'Network or manifest error',
        severity: 'error',
      });
    } finally {
      busyId = null;
    }
  }

  function handleCoverError(card: StoreCardModel) {
    if (card.fallbackUrl && !coverFallbackUsed[card.id]) {
      coverFallbackUsed = { ...coverFallbackUsed, [card.id]: true };
    } else {
      coverFailed = { ...coverFailed, [card.id]: true };
    }
  }
</script>

<div class="space-y-4">
  {#if pluginManager.safeMode}
    <div class="rounded-[12px] border border-[var(--lw-warn)]/40 bg-[var(--lw-warn)]/12 px-4 py-3 text-[12.5px] text-[var(--lw-warn)]">
      Started in <strong>--safe-mode</strong>. All plugins are temporarily suspended.
    </div>
  {/if}

  <!-- Engine Safety Bar -->
  <div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[rgba(255,255,255,0.06)] border-t-[rgba(255,255,255,0.1)] border-b-[rgba(0,0,0,0.35)] bg-[var(--lw-bg-elevated,var(--lw-surface-raised))] px-4 py-3">
    <div>
      <div class="text-[13.5px] font-semibold">
        Plugin Storefront ({activeCount} active)
      </div>
      <div class="text-[11.5px] text-[var(--lw-text-secondary)]">
        Official & pinned Community mods · API v1 sandbox · stored in app data
      </div>
    </div>

    <label class="flex cursor-pointer items-center gap-2 text-[12px] font-semibold text-[var(--lw-danger)]">
      <input
        type="checkbox"
        checked={pluginManager.allDisabled}
        onchange={(e) => void pluginManager.setAllDisabled(e.currentTarget.checked)}
        class="h-4 w-4 accent-[var(--lw-danger)]"
      />
      Disable All Plugins
    </label>
  </div>

  <!-- Store Header & Navigation Bar -->
  <div
    data-lw="plugin-store-header"
    class="space-y-3 rounded-[14px] border border-[rgba(255,255,255,0.06)] border-t-[rgba(255,255,255,0.1)] border-b-[rgba(0,0,0,0.35)] bg-[var(--lw-bg-elevated,var(--lw-surface-raised))] p-3.5"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex flex-wrap items-center gap-1.5">
        <Buttons variant="chip" size="sm" active={activeTab === 'discover'} onclick={() => (activeTab = 'discover')}>
          Discover
        </Buttons>
        <Buttons variant="chip" size="sm" active={activeTab === 'palette'} onclick={() => (activeTab = 'palette')}>
          Palettes
        </Buttons>
        <Buttons variant="chip" size="sm" active={activeTab === 'theme'} onclick={() => (activeTab = 'theme')}>
          Themes
        </Buttons>
        <Buttons variant="chip" size="sm" active={activeTab === 'snippet'} onclick={() => (activeTab = 'snippet')}>
          Snippets
        </Buttons>
        <Buttons variant="chip" size="sm" active={activeTab === 'widget'} onclick={() => (activeTab = 'widget')}>
          Widgets
        </Buttons>
        <Buttons variant="chip" size="sm" active={activeTab === 'installed'} onclick={() => (activeTab = 'installed')}>
          Installed ({pluginManager.plugins.length})
        </Buttons>
      </div>

      <div class="flex items-center gap-2">
        <Buttons
          variant="ghost"
          size="sm"
          disabled={pluginManager.marketplaceLoading}
          onclick={() => void pluginManager.fetchMarketplace()}
          title="Refresh registry"
        >
          <WaveIcon name="refresh" size={14} class={pluginManager.marketplaceLoading ? 'animate-spin' : ''} />
          Refresh
        </Buttons>
        <Buttons variant="secondary" size="sm" onclick={() => (githubModalOpen = true)}>
          <WaveIcon name="plus" size={14} />
          Install from GitHub
        </Buttons>
      </div>
    </div>

    <div class="flex flex-wrap items-center justify-between gap-2.5">
      <div class="min-w-[220px] flex-1">
        <Input bind:value={search} placeholder="Search name, tags, or @github..." />
      </div>

      <div class="flex flex-wrap items-center gap-2">
        {#if activeTab !== 'installed'}
          <div class="flex items-center gap-1 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-0.5">
            {#each ([['all', 'All'], ['official', 'Official'], ['external', 'Community']] as const) as [val, label] (val)}
              <button
                type="button"
                onclick={() => (sourceFilter = val)}
                class="cursor-pointer rounded-[8px] px-2.5 py-1 text-[11.5px] font-medium transition-colors duration-[160ms] {sourceFilter === val
                  ? 'bg-[var(--lw-accent)] font-semibold text-[var(--lw-on-accent)]'
                  : 'text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
              >
                {label}
              </button>
            {/each}
          </div>
        {/if}

        <Select
          bind:value={sortOrder}
          options={[
            { value: 'featured', label: 'Sort: Featured' },
            { value: 'recent', label: 'Recently Updated' },
            { value: 'alpha', label: 'Alphabetical' },
          ]}
        />
      </div>
    </div>
  </div>

  {#if pluginManager.marketplaceError}
    <div class="rounded-[12px] border border-[var(--lw-danger)]/35 bg-[var(--lw-danger)]/12 px-4 py-2.5 text-[12px] text-[var(--lw-danger)]">
      {pluginManager.marketplaceError}
    </div>
  {/if}

  {#if pluginManager.marketplaceLoading && visibleCards.length === 0}
    <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] py-12 text-center text-[13px] text-[var(--lw-text-secondary)]">
      Loading Clash-Projects/lastwave-plugins...
    </div>
  {:else if visibleCards.length === 0}
    <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] py-12 text-center text-[13px] text-[var(--lw-text-secondary)]">
      {#if activeTab !== 'installed' && pluginManager.marketplace.length === 0 && !search.trim()}
        No plugins published in the registry yet. Install one via GitHub URL above.
      {:else if activeTab === 'installed' && pluginManager.plugins.length === 0 && !search.trim()}
        No plugins installed yet.
      {:else}
        No matching plugins found.
      {/if}
    </div>
  {:else}
    <!-- Responsive Store Grid -->
    <div class="grid gap-4" style="grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));">
      {#each visibleCards as card (card.key)}
        {@const imgSrc = coverFallbackUsed[card.id] ? card.fallbackUrl : card.coverUrl}
        <div
          data-lw="plugin-store-card"
          onclick={() => openDetail(card)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              openDetail(card);
            }
          }}
          tabindex="0"
          role="button"
          class="group flex cursor-pointer flex-col overflow-hidden rounded-[14px] border border-[rgba(255,255,255,0.06)] border-t-[rgba(255,255,255,0.1)] border-b-[rgba(0,0,0,0.35)] bg-[var(--lw-bg-elevated,var(--lw-surface-raised))] text-left shadow-[inset_0_1px_0_rgba(255,255,255,0.05),0_6px_18px_rgba(0,0,0,0.28)] transition-all duration-[160ms] ease-[cubic-bezier(0.22,1,0.36,1)] hover:-translate-y-[2px] hover:border-t-[rgba(255,255,255,0.16)] hover:shadow-[inset_0_1px_0_rgba(255,255,255,0.08),0_12px_26px_rgba(0,0,0,0.42)]"
        >
          <!-- 16:10 Cover Thumbnail -->
          <div class="relative aspect-[16/10] w-full overflow-hidden bg-[var(--lw-bg-deep)]">
            {#if imgSrc && !coverFailed[card.id]}
              <img
                src={imgSrc}
                alt="{card.name} preview"
                loading="lazy"
                draggable="false"
                onerror={() => handleCoverError(card)}
                class="h-full w-full select-none object-cover transition-transform duration-[220ms] ease-[cubic-bezier(0.22,1,0.36,1)] group-hover:scale-[1.02]"
              />
            {:else if card.swatch}
              <div class="flex h-full w-full">
                {#each card.swatch as hex (hex)}
                  <div class="h-full flex-1" style:background={hex}></div>
                {/each}
              </div>
            {:else}
              <div class="flex h-full w-full items-center justify-center bg-gradient-to-br from-white/5 to-transparent text-[var(--lw-text-tertiary)]">
                <WaveIcon name={typeIcon(card.type)} size={32} />
              </div>
            {/if}

            <div class="absolute right-2.5 top-2.5 flex items-center gap-1.5">
              <span class="rounded-[6px] border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.4px] {typeBadgeClass(card.type)}">
                {typeLabel(card.type)}
              </span>
            </div>

            {#if card.source === 'external'}
              <span class="absolute left-2.5 top-2.5 rounded-[6px] bg-black/70 px-2 py-0.5 text-[10px] font-medium text-white/85">
                Community
              </span>
            {/if}

            {#if card.previews.length > 1}
              <span class="absolute bottom-2 right-2 rounded-full bg-black/72 px-2 py-0.5 text-[10.5px] font-medium text-white/90">
                1 / {card.previews.length}
              </span>
            {/if}
          </div>

          <!-- Card Body -->
          <div class="flex flex-1 flex-col p-3.5">
            <div class="flex items-center justify-between gap-2">
              <h3 class="truncate text-[14px] font-bold tracking-[-0.015em] text-[var(--lw-text-primary)]">
                {card.name}
              </h3>
              {#if card.official}
                <span
                  title="Official verified plugin"
                  class="inline-flex shrink-0 items-center gap-1 rounded-full bg-[var(--lw-accent)]/15 px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-accent)]"
                >
                  <WaveIcon name="check" size={10} />
                  Official
                </span>
              {/if}
            </div>

            <div class="mt-0.5 truncate text-[11.5px] text-[var(--lw-text-tertiary)]">
              by @{card.authorGithub}
            </div>

            <p class="mt-2 line-clamp-2 flex-1 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
              {card.description}
            </p>

            {#if card.installedRecord?.error}
              <div class="mt-2 truncate text-[11px] text-[var(--lw-danger)]">
                {card.installedRecord.error}
              </div>
            {/if}

            <!-- Footer Action Row -->
            <div class="mt-3 flex items-center justify-between gap-2 border-t border-white/6 pt-2.5">
              <span class="rounded-[6px] bg-white/6 px-2 py-0.5 font-mono text-[11px] text-[var(--lw-text-tertiary)]">
                v{card.version}
              </span>

              <div class="flex items-center gap-1.5">
                {#if card.installedRecord && !card.installedRecord.builtin && activeTab === 'installed'}
                  <button
                    type="button"
                    onclick={(e) => {
                      e.stopPropagation();
                      void pluginManager.uninstallPlugin(card.id);
                    }}
                    title="Uninstall"
                    class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-danger)]"
                  >
                    <WaveIcon name="trash" size={14} />
                  </button>
                {/if}
                <Buttons
                  variant={card.installedRecord?.enabled && !card.hasUpdate ? 'secondary' : 'primary'}
                  size="sm"
                  disabled={busyId === card.id || pluginManager.safeMode || pluginManager.allDisabled}
                  onclick={(e) => void handleCardAction(e, card)}
                >
                  {#if busyId === card.id}
                    Working...
                  {:else if card.hasUpdate}
                    Update
                  {:else if card.installedRecord}
                    {card.installedRecord.enabled ? 'Active' : 'Enable'}
                  {:else}
                    Install
                  {/if}
                </Buttons>
              </div>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<GithubInstallModal
  open={githubModalOpen}
  onClose={() => (githubModalOpen = false)}
  onInstalled={() => (activeTab = 'installed')}
/>

<PluginDetailModal
  selection={detailSelection}
  onClose={() => (detailSelection = null)}
  onSelectTag={(tag) => {
    search = tag;
    if (activeTab === 'installed') activeTab = 'discover';
  }}
/>
