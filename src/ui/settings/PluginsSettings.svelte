<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let activeCount = $derived(pluginManager.plugins.filter((p) => p.enabled).length);
</script>

<div class="space-y-4">
  <div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
    <div>
      <div class="text-[14px] font-semibold">UI Plugin Engine ({activeCount} active)</div>
      <p class="text-[12px] text-[var(--lw-text-secondary)]">
        Manage Palettes, Themes, Snippets, and Widgets in the Store.
      </p>
    </div>
    <Buttons variant="primary" size="sm" onclick={() => router.navigate('/store')}>Open Store</Buttons>
  </div>

  <div class="space-y-2">
    {#each pluginManager.plugins as rec (rec.manifest.id)}
      <div class="flex items-center justify-between gap-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5">
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <span class="truncate text-[13px] font-semibold">{rec.manifest.name}</span>
            <span class="rounded-[5px] bg-[var(--lw-surface-overlay)] px-1.5 py-0.5 text-[10px] text-[var(--lw-text-secondary)] uppercase">
              {rec.manifest.type}
            </span>
          </div>
          <div class="truncate text-[11.5px] text-[var(--lw-text-tertiary)]">{rec.manifest.description}</div>
        </div>
        <Buttons
          variant={rec.enabled ? 'primary' : 'secondary'}
          size="sm"
          onclick={() => pluginManager.togglePlugin(rec.manifest.id)}
        >
          {rec.enabled ? 'On' : 'Off'}
        </Buttons>
      </div>
    {/each}
  </div>
</div>
