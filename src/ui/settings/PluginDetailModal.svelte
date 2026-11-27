<script lang="ts">
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import {
    extractAuthorGithub,
    formatAuthorName,
    isOfficialPlugin,
    normalizePreviews,
    resolveGithubBrowseUrl,
    resolvePluginAssetUrl,
    type AssetSourceRef,
  } from '$src/lib/plugins/resolver';
  import { parsePaletteTokens } from '$src/lib/plugins/themeLoader';
  import type {
    HydratedPluginDetails,
    MarketplacePluginEntry,
    PluginRecord,
    PluginType,
  } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  export type StoreDetailSelection =
    | { kind: 'market'; entry: MarketplacePluginEntry }
    | { kind: 'installed'; record: PluginRecord };

  interface Props {
    selection: StoreDetailSelection | null;
    onClose: () => void;
    onSelectTag?: (tag: string) => void;
  }

  let { selection, onClose, onSelectTag }: Props = $props();

  let activeSlide = $state(0);
  let busy = $state(false);
  let failedImgs = $state<Record<number, boolean>>({});
  let fallbackImgs = $state<Record<number, boolean>>({});

  let marketEntry = $derived(selection?.kind === 'market' ? selection.entry : null);
  let installedRecord = $derived.by(() => {
    if (!selection) return null;
    if (selection.kind === 'installed') return selection.record;
    return pluginManager.plugins.find((p) => p.manifest.id === selection.entry.id) ?? null;
  });

  let itemId = $derived(marketEntry?.id ?? installedRecord?.manifest.id ?? '');
  let detailCacheKey = $derived(
    marketEntry
      ? `${marketEntry.id}@${marketEntry.commit || marketEntry.version}`
      : installedRecord
        ? `${installedRecord.manifest.id}@${installedRecord.manifest.commit || installedRecord.manifest.version}`
        : ''
  );

  let hydrated = $derived<HydratedPluginDetails | undefined>(
    detailCacheKey ? pluginManager.detailsCache[detailCacheKey] : undefined
  );

  $effect(() => {
    if (!selection) return;
    activeSlide = 0;
    failedImgs = {};
    fallbackImgs = {};
    void pluginManager.hydratePluginDetails(
      selection.kind === 'market' ? selection.entry : selection.record
    );
  });

  let manifest = $derived(hydrated?.manifest ?? installedRecord?.manifest);
  let name = $derived(marketEntry?.name ?? manifest?.name ?? '');
  let type = $derived<PluginType>(marketEntry?.type ?? manifest?.type ?? 'theme');
  let version = $derived(marketEntry?.version ?? manifest?.version ?? '1.0.0');
  let updatedAt = $derived(marketEntry?.updatedAt ?? manifest?.updatedAt ?? '2026-11-27');
  let description = $derived(marketEntry?.description ?? manifest?.description ?? '');
  let authorRaw = $derived(marketEntry?.author ?? manifest?.author);
  let authorName = $derived(formatAuthorName(authorRaw));
  let authorGithub = $derived(extractAuthorGithub(authorRaw));
  let source = $derived(marketEntry?.source ?? manifest?.source);
  let official = $derived(isOfficialPlugin({ source, author: authorRaw }));
  let tags = $derived(
    (marketEntry?.tags?.length ? marketEntry.tags : manifest?.tags) ?? []
  );

  let assetRef = $derived<AssetSourceRef>({
    source: marketEntry?.source ?? manifest?.source,
    path: marketEntry?.path ?? manifest?.path,
    repo: marketEntry?.repo ?? manifest?.repo,
    commit: marketEntry?.commit ?? manifest?.commit,
    subpath: marketEntry?.subpath ?? manifest?.subpath,
  });

  let previewPaths = $derived(
    normalizePreviews({
      previews: marketEntry?.previews?.length ? marketEntry.previews : manifest?.previews,
      preview: manifest?.preview,
    })
  );

  let previewUrls = $derived(
    previewPaths.map((rel, idx) =>
      resolvePluginAssetUrl(assetRef, rel, Boolean(fallbackImgs[idx]))
    )
  );

  let hasUsableScreenshots = $derived(
    previewUrls.some((url, idx) => Boolean(url) && !failedImgs[idx])
  );

  let paletteTokens = $derived.by(() => {
    const tokens =
      hydrated?.paletteTokens ??
      (installedRecord?.cssCode && type === 'palette'
        ? parsePaletteTokens(installedRecord.cssCode)
        : {});
    return tokens;
  });

  let swatchChips = $derived.by(() => {
    const fallbackSwatch = marketEntry?.swatch ?? manifest?.swatch;
    const primary = paletteTokens['--lw-bg'] ?? paletteTokens['--lw-bg-base'] ?? fallbackSwatch?.[0];
    const surface =
      paletteTokens['--lw-surface-raised'] ??
      paletteTokens['--lw-bg-elevated'] ??
      paletteTokens['--lw-surface'] ??
      fallbackSwatch?.[1];
    const text = paletteTokens['--lw-text-primary'] ?? '#f5f4f0';
    const accent = paletteTokens['--lw-accent'] ?? fallbackSwatch?.[2];
    const chips: Array<{ label: string; token: string; value: string }> = [];
    if (primary) chips.push({ label: 'Primary', token: '--lw-bg', value: primary });
    if (surface) chips.push({ label: 'Surface', token: '--lw-bg-elevated', value: surface });
    if (text) chips.push({ label: 'Text', token: '--lw-text-primary', value: text });
    if (accent) chips.push({ label: 'Accent', token: '--lw-accent', value: accent });
    return chips;
  });

  let readmeBlocks = $derived.by(() => {
    const text = hydrated?.readme?.trim();
    if (!text) return [];
    return text.split(/\r?\n/).map((line) => {
      const trimmed = line.trim();
      if (trimmed.startsWith('## ')) return { kind: 'h2' as const, text: trimmed.slice(3) };
      if (trimmed.startsWith('# ')) return { kind: 'h1' as const, text: trimmed.slice(2) };
      if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
        return { kind: 'li' as const, text: trimmed.slice(2) };
      }
      if (!trimmed) return { kind: 'spacer' as const, text: '' };
      return { kind: 'p' as const, text: trimmed };
    });
  });

  let provenanceLabel = $derived.by(() => {
    if (installedRecord?.builtin) return 'Built-in Preset';
    if (source === 'official') return 'Official Registry (Clash-Projects/lastwave-plugins)';
    if (assetRef.repo && assetRef.commit) {
      return `${assetRef.repo}@${assetRef.commit.slice(0, 7)}`;
    }
    return manifest?.sourceUrl || 'Local Install';
  });

  let entryFileName = $derived(
    marketEntry?.entry ??
      manifest?.entry ??
      (type === 'palette'
        ? 'colors.json'
        : type === 'theme'
          ? 'theme.css'
          : type === 'snippet'
            ? 'snippet.css'
            : 'widget.js')
  );

  let githubBrowseUrl = $derived(
    resolveGithubBrowseUrl({ ...assetRef, sourceUrl: manifest?.sourceUrl })
  );

  let hasUpdate = $derived(
    Boolean(marketEntry && installedRecord && installedRecord.manifest.version !== marketEntry.version)
  );

  function prevSlide() {
    if (previewUrls.length <= 1) return;
    activeSlide = (activeSlide - 1 + previewUrls.length) % previewUrls.length;
  }

  function nextSlide() {
    if (previewUrls.length <= 1) return;
    activeSlide = (activeSlide + 1) % previewUrls.length;
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (!selection) return;
    if (e.key === 'Escape') {
      onClose();
    } else if (e.key === 'ArrowLeft') {
      prevSlide();
    } else if (e.key === 'ArrowRight') {
      nextSlide();
    }
  }

  function handleImageError(idx: number) {
    if (source === 'official' && !fallbackImgs[idx]) {
      fallbackImgs = { ...fallbackImgs, [idx]: true };
    } else {
      failedImgs = { ...failedImgs, [idx]: true };
    }
  }

  async function handlePrimaryAction() {
    if (busy) return;
    busy = true;
    try {
      if (installedRecord && !hasUpdate) {
        await pluginManager.togglePlugin(itemId);
      } else if (marketEntry) {
        await pluginManager.installFromRegistry(marketEntry);
      }
    } catch (err) {
      featuresStore.showToast({
        title: `Action failed for ${name}`,
        subtitle: err instanceof Error ? err.message : 'Unexpected error',
        severity: 'error',
      });
    } finally {
      busy = false;
    }
  }

  async function handleUninstall() {
    if (!installedRecord || installedRecord.builtin || busy) return;
    busy = true;
    try {
      await pluginManager.uninstallPlugin(installedRecord.manifest.id);
      if (selection?.kind === 'installed') onClose();
    } finally {
      busy = false;
    }
  }

  function copyHex(value: string) {
    void navigator.clipboard?.writeText(value);
    featuresStore.showToast({ title: `Copied ${value}`, severity: 'info' });
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if selection}
  <div
    data-lw="plugin-detail-view"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 md:p-6"
    role="dialog"
    aria-modal="true"
    aria-label={name}
  >
    <button
      type="button"
      aria-label="Close detail view"
      onclick={onClose}
      class="absolute inset-0 cursor-default"
    ></button>

    <div
      class="relative z-10 flex max-h-[90vh] w-full max-w-[820px] flex-col overflow-hidden rounded-[18px] border border-[rgba(255,255,255,0.08)] border-t-[rgba(255,255,255,0.14)] border-b-[rgba(0,0,0,0.5)] bg-[var(--lw-bg-elevated,var(--lw-surface-raised))] shadow-[0_28px_64px_rgba(0,0,0,0.65)]"
    >
      <div class="flex items-center justify-between border-b border-[var(--lw-divider)] px-5 py-3.5">
        <div class="flex items-center gap-2 text-[12px] text-[var(--lw-text-secondary)]">
          <span class="rounded-[6px] bg-white/8 px-2 py-0.5 text-[10.5px] font-semibold uppercase tracking-[0.5px] text-[var(--lw-text-primary)]">
            {type}
          </span>
          <span class="rounded-[6px] bg-[var(--lw-success)]/12 px-2 py-0.5 text-[10.5px] font-medium text-[var(--lw-success)]">
            Compatible · API v1
          </span>
        </div>

        <button
          type="button"
          onclick={onClose}
          title="Close (Esc)"
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] transition-colors duration-[160ms] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="close" size={16} />
        </button>
      </div>

      <div class="lw-scroll flex-1 space-y-5 overflow-y-auto p-5 md:p-6">
        <!-- Multi-Image Hero Gallery / Palette Swatch Stage -->
        <div class="space-y-2.5">
          {#if hasUsableScreenshots}
            {@const currentUrl = previewUrls[activeSlide]}
            <div class="group relative aspect-[16/10] w-full overflow-hidden rounded-[14px] border border-[rgba(255,255,255,0.08)] bg-[var(--lw-bg-deep)]">
              {#if currentUrl && !failedImgs[activeSlide]}
                <img
                  src={currentUrl}
                  alt="{name} preview {activeSlide + 1}"
                  draggable="false"
                  onerror={() => handleImageError(activeSlide)}
                  class="h-full w-full select-none object-cover"
                />
              {/if}

              {#if previewUrls.length > 1}
                <button
                  type="button"
                  onclick={prevSlide}
                  title="Previous image (Left Arrow)"
                  class="absolute left-3 top-1/2 flex h-9 w-9 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border border-white/15 bg-black/65 text-white opacity-0 transition-opacity duration-[160ms] group-hover:opacity-100 hover:bg-black/85"
                >
                  <WaveIcon name="chevronLeft" size={18} />
                </button>
                <button
                  type="button"
                  onclick={nextSlide}
                  title="Next image (Right Arrow)"
                  class="absolute right-3 top-1/2 flex h-9 w-9 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border border-white/15 bg-black/65 text-white opacity-0 transition-opacity duration-[160ms] group-hover:opacity-100 hover:bg-black/85"
                >
                  <WaveIcon name="chevronRight" size={18} />
                </button>
                <div class="absolute bottom-3 right-3 rounded-full bg-black/70 px-2.5 py-0.5 text-[11px] font-medium text-white/90">
                  {activeSlide + 1} / {previewUrls.length}
                </div>
              {/if}
            </div>

            {#if previewUrls.length > 1}
              <div class="lw-scroll flex gap-2 overflow-x-auto pb-1">
                {#each previewUrls as thumbUrl, idx (`${thumbUrl}-${idx}`)}
                  {#if thumbUrl && !failedImgs[idx]}
                    <button
                      type="button"
                      onclick={() => (activeSlide = idx)}
                      class="aspect-[16/10] w-[96px] shrink-0 cursor-pointer overflow-hidden rounded-[8px] border transition-all duration-[160ms] {activeSlide === idx
                        ? 'border-[var(--lw-accent)] ring-2 ring-[var(--lw-accent)]'
                        : 'border-white/10 opacity-70 hover:opacity-100'}"
                    >
                      <img
                        src={thumbUrl}
                        alt="{name} thumbnail {idx + 1}"
                        loading="lazy"
                        draggable="false"
                        onerror={() => handleImageError(idx)}
                        class="h-full w-full select-none object-cover"
                      />
                    </button>
                  {/if}
                {/each}
              </div>
            {/if}
          {:else if type === 'palette' && swatchChips.length > 0}
            <div class="rounded-[14px] border border-[rgba(255,255,255,0.08)] bg-[var(--lw-bg-deep)] p-4">
              <div class="mb-3 text-[11px] font-semibold uppercase tracking-[0.7px] text-[var(--lw-text-tertiary)]">
                Interactive Palette Tokens
              </div>
              <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
                {#each swatchChips as chip (chip.token)}
                  <button
                    type="button"
                    onclick={() => copyHex(chip.value)}
                    title="Click to copy {chip.value}"
                    class="flex cursor-pointer flex-col overflow-hidden rounded-[10px] border border-white/10 bg-[var(--lw-surface)] text-left transition-transform duration-[160ms] ease-[var(--ease-native)] hover:-translate-y-0.5"
                  >
                    <div class="h-16 w-full" style:background={chip.value}></div>
                    <div class="p-2.5">
                      <div class="text-[12px] font-semibold">{chip.label}</div>
                      <div class="font-mono text-[11px] text-[var(--lw-text-secondary)]">{chip.value}</div>
                    </div>
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          {#if hasUsableScreenshots && type === 'palette' && swatchChips.length > 0}
            <div class="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              {#each swatchChips as chip (chip.token)}
                <button
                  type="button"
                  onclick={() => copyHex(chip.value)}
                  class="flex cursor-pointer items-center gap-2.5 rounded-[10px] border border-white/8 bg-[var(--lw-surface)] px-3 py-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
                >
                  <span class="h-6 w-6 shrink-0 rounded-[6px] border border-white/15" style:background={chip.value}></span>
                  <div class="min-w-0">
                    <div class="text-[11.5px] font-semibold">{chip.label}</div>
                    <div class="truncate font-mono text-[10.5px] text-[var(--lw-text-tertiary)]">{chip.value}</div>
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Header Block -->
        <div class="flex flex-wrap items-start justify-between gap-4 border-b border-[var(--lw-divider)] pb-4">
          <div class="min-w-0 flex-1">
            <h2 class="text-[22px] font-bold tracking-[-0.025em]">{name}</h2>
            <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-[var(--lw-text-secondary)]">
              <span>
                By <strong class="font-semibold text-[var(--lw-text-primary)]">{authorName}</strong>
                (<a
                  href="https://github.com/{authorGithub}"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-[var(--lw-accent)] hover:underline"
                >@{authorGithub}</a>)
              </span>
              <span>·</span>
              <span>v{version}</span>
              <span>·</span>
              <span>Updated {updatedAt}</span>
            </div>
          </div>

          <div class="flex shrink-0 items-center gap-2">
            {#if installedRecord && !installedRecord.builtin}
              <Buttons variant="danger" size="sm" disabled={busy} onclick={handleUninstall}>
                Uninstall
              </Buttons>
            {/if}
            <Buttons
              variant={installedRecord?.enabled && !hasUpdate ? 'secondary' : 'primary'}
              size="md"
              disabled={busy || pluginManager.safeMode || pluginManager.allDisabled}
              onclick={handlePrimaryAction}
            >
              {#if busy}
                Working...
              {:else if hasUpdate}
                Update to v{marketEntry?.version}
              {:else if installedRecord}
                {installedRecord.enabled ? 'Disable' : 'Enable'}
              {:else}
                Install
              {/if}
            </Buttons>
          </div>
        </div>

        <!-- Full Description & Hydrated README Notes -->
        <div class="space-y-3">
          <p class="text-[13.5px] leading-relaxed text-[var(--lw-text-primary)]">{description}</p>

          {#if hydrated?.loading}
            <div class="text-[12px] text-[var(--lw-text-tertiary)]">Loading README & manifest details...</div>
          {:else if readmeBlocks.length > 0}
            <div class="space-y-1.5 rounded-[12px] border border-white/6 bg-[var(--lw-bg)]/55 p-4 text-[12.5px] leading-relaxed text-[var(--lw-text-secondary)]">
              {#each readmeBlocks as block, i (`${i}-${block.text}`)}
                {#if block.kind === 'h1' || block.kind === 'h2'}
                  <div class="pt-1 text-[13.5px] font-semibold text-[var(--lw-text-primary)]">{block.text}</div>
                {:else if block.kind === 'li'}
                  <div class="flex gap-2 pl-1">
                    <span class="text-[var(--lw-accent)]">•</span>
                    <span>{block.text}</span>
                  </div>
                {:else if block.kind === 'spacer'}
                  <div class="h-1"></div>
                {:else}
                  <p>{block.text}</p>
                {/if}
              {/each}
            </div>
          {/if}

          {#if tags.length > 0}
            <div class="flex flex-wrap gap-1.5 pt-1">
              {#each tags as tag (tag)}
                <button
                  type="button"
                  onclick={() => {
                    onSelectTag?.(tag);
                    onClose();
                  }}
                  class="cursor-pointer rounded-full border border-white/8 bg-white/5 px-2.5 py-0.5 text-[11px] font-medium text-[var(--lw-text-secondary)] transition-colors hover:border-[var(--lw-accent)]/50 hover:text-[var(--lw-text-primary)]"
                >
                  #{tag}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Source & Provenance Footer -->
        <div class="flex flex-wrap items-center justify-between gap-3 rounded-[12px] border border-white/6 bg-[var(--lw-bg)]/65 px-4 py-3 text-[12px] text-[var(--lw-text-secondary)]">
          <div class="space-y-0.5">
            <div>
              <span class="text-[var(--lw-text-tertiary)]">Source:</span>
              <span class="ml-1 font-mono text-[11.5px] text-[var(--lw-text-primary)]">{provenanceLabel}</span>
            </div>
            <div>
              <span class="text-[var(--lw-text-tertiary)]">Included files:</span>
              <span class="ml-1 font-mono text-[11.5px]">manifest.json + {entryFileName}</span>
            </div>
          </div>

          <a
            href={githubBrowseUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="inline-flex items-center gap-1.5 rounded-[8px] border border-white/10 bg-[var(--lw-surface)] px-3 py-1.5 text-[12px] font-medium text-[var(--lw-text-primary)] transition-colors hover:bg-[var(--lw-wash-hover)]"
          >
            View on GitHub
            <WaveIcon name="external" size={13} />
          </a>
        </div>
      </div>
    </div>
  </div>
{/if}
