<script lang="ts">
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import type { PluginType } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import WaveEntrance from '$src/ui/theme/WaveEntrance.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  type StoreTab = 'all' | 'palette' | 'theme' | 'snippet' | 'widget' | 'import';

  let tab = $state<StoreTab>('all');
  let query = $state('');

  let newName = $state('');
  let newAuthor = $state('Community');
  let newType = $state<PluginType>('palette');
  let newCode = $state('');

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return pluginManager.plugins.filter((rec) => {
      if (tab !== 'all' && tab !== 'import' && rec.manifest.type !== tab) return false;
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

  async function installManual(e: SubmitEvent) {
    e.preventDefault();
    if (!newName.trim() || !newCode.trim()) return;
    const id = `${newType}-${newName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${Date.now().toString(36)}`;
    const isJs = newType === 'widget';
    await pluginManager.installPlugin(
      {
        id,
        name: newName.trim(),
        version: '1.0.0',
        author: newAuthor.trim() || 'Community',
        description: `Custom ${typeLabel(newType).toLowerCase()}`,
        type: newType,
        css: isJs ? undefined : `${newType}.css`,
        entry: isJs ? 'index.js' : undefined,
      },
      isJs ? { sourceCode: newCode } : { cssCode: newCode }
    );
    featuresStore.showToast({
      title: `Installed ${newName.trim()}`,
      subtitle: `${typeLabel(newType)} activated`,
      severity: 'success',
    });
    newName = '';
    newCode = '';
    tab = newType;
  }
</script>

<div class="w-full space-y-6 px-4 pt-[22px] pb-12 md:px-6 xl:px-7">
  <WaveEntrance index={0}>
    <header class="flex flex-wrap items-end justify-between gap-4">
      <div>
        <div class="text-[10px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          COMMUNITY STORE
        </div>
        <h1 class="mt-0.5 text-[24px] font-bold tracking-[-0.4px]">UI Plugins</h1>
        <p class="mt-0.5 text-[12.5px] text-[var(--lw-text-secondary)]">
          Palettes, Themes, Snippets, and Widgets — 100% UI-only customization with instant hot-swap.
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
        <div class="w-[220px]">
          <Input bind:value={query} placeholder="Search store..." />
        </div>
      </div>
    </header>
  </WaveEntrance>

  <WaveEntrance index={1}>
    <div class="flex flex-wrap items-center gap-1.5 border-b border-[var(--lw-divider)] pb-3">
      <Buttons variant="chip" size="sm" active={tab === 'all'} onclick={() => (tab = 'all')}>All</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'palette'} onclick={() => (tab = 'palette')}>Palettes</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'theme'} onclick={() => (tab = 'theme')}>Themes</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'snippet'} onclick={() => (tab = 'snippet')}>Snippets</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'widget'} onclick={() => (tab = 'widget')}>Widgets</Buttons>
      <Buttons variant="chip" size="sm" active={tab === 'import'} onclick={() => (tab = 'import')}>
        <WaveIcon name="plus" size={13} /> Import Custom
      </Buttons>
    </div>
  </WaveEntrance>

  {#if tab === 'import'}
    <form onsubmit={installManual} class="max-w-[760px] space-y-3 rounded-[18px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-5">
      <div class="text-[15px] font-bold">Import UI Plugin</div>
      <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
        <Input bind:value={newName} placeholder="Plugin name" />
        <Input bind:value={newAuthor} placeholder="Author" />
        <Select
          bind:value={newType}
          options={[
            { value: 'palette', label: 'Palette (colors CSS)' },
            { value: 'theme', label: 'Theme (layout CSS)' },
            { value: 'snippet', label: 'Snippet (tweak CSS)' },
            { value: 'widget', label: 'Widget (UI JS)' },
          ]}
        />
      </div>
      <textarea
        bind:value={newCode}
        rows="8"
        placeholder={newType === 'widget'
          ? 'export function init(api) {\n  return api.ui.registerPlayerBarControl({ id: "my-w", label: "My Widget", icon: "sparkles", onClick: () => api.ui.showToast("Hi!") }, "my-widget");\n}'
          : ':root {\n  --lw-accent: #7be3a8;\n}'}
        class="w-full rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-bg)] p-3 font-mono text-[12px] text-[var(--lw-text-primary)] focus:border-[var(--lw-accent)] focus:outline-none"
      ></textarea>
      <div class="flex justify-end">
        <Buttons type="submit" variant="primary" size="sm">Install & Activate</Buttons>
      </div>
    </form>
  {:else}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
      {#each filtered as rec (rec.manifest.id)}
        <div class="flex flex-col justify-between rounded-[18px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4 transition-colors hover:border-[var(--lw-outline)]">
          <div>
            {#if rec.manifest.swatch}
              <div class="mb-3 flex h-14 overflow-hidden rounded-[12px] border border-white/10">
                <div class="flex-[5]" style:background={rec.manifest.swatch[0]}></div>
                <div class="flex-[3]" style:background={rec.manifest.swatch[1]}></div>
                <div class="flex-[2]" style:background={rec.manifest.swatch[2]}></div>
              </div>
            {:else}
              <div class="mb-3 flex h-11 items-center justify-between rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-raised)] px-3.5">
                <WaveIcon
                  name={rec.manifest.type === 'theme'
                    ? 'palette'
                    : rec.manifest.type === 'snippet'
                      ? 'wand'
                      : 'plugin'}
                  size={16}
                  class="text-[var(--lw-accent)]"
                />
                <span class="text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
                  {typeLabel(rec.manifest.type)}
                </span>
              </div>
            {/if}

            <div class="flex items-center justify-between gap-2">
              <h2 class="truncate text-[14.5px] font-bold">{rec.manifest.name}</h2>
              <span class="rounded-[6px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] font-semibold text-[var(--lw-text-secondary)]">
                {typeLabel(rec.manifest.type)}
              </span>
            </div>
            <p class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">{rec.manifest.description}</p>
            {#if rec.error}
              <div class="mt-2 rounded-[8px] bg-[var(--lw-danger)]/15 px-2.5 py-1 text-[11.5px] text-[var(--lw-danger)]">
                {rec.error}
              </div>
            {/if}
          </div>

          <div class="mt-4 flex items-center justify-between border-t border-[var(--lw-divider)] pt-3">
            <span class="text-[11px] text-[var(--lw-text-tertiary)]">by {rec.manifest.author} · v{rec.manifest.version}</span>
            <div class="flex items-center gap-1.5">
              <Buttons
                variant={rec.enabled ? 'primary' : 'secondary'}
                size="sm"
                onclick={() => pluginManager.togglePlugin(rec.manifest.id)}
              >
                {rec.enabled
                  ? rec.manifest.type === 'palette' || rec.manifest.type === 'theme'
                    ? 'Active'
                    : 'Enabled'
                  : 'Apply'}
              </Buttons>
              {#if !rec.builtin}
                <button
                  type="button"
                  onclick={() => pluginManager.uninstallPlugin(rec.manifest.id)}
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
