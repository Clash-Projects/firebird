<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
</script>

<div class="mx-auto max-w-[720px] px-6 py-4">
  <CommandBar title="Customize Home Layout" subtitle="Reorder or hide shelves shown on Home">
    {#snippet actions()}
      <Buttons variant="secondary" onclick={() => router.back()}>Done</Buttons>
    {/snippet}
  </CommandBar>

  <div class="mt-4 space-y-2">
    {#each settingsStore.homeShelves as shelf, i (shelf.id)}
      <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-4 py-3">
        <label class="flex cursor-pointer items-center gap-3">
          <input
            type="checkbox"
            checked={shelf.visible}
            onchange={() => settingsStore.toggleShelf(shelf.id)}
            class="h-4 w-4 accent-[var(--lw-accent)]"
          />
          <span class="text-[13.5px] font-semibold">{shelf.label}</span>
        </label>
        <div class="flex items-center gap-1">
          <button
            type="button"
            disabled={i === 0}
            onclick={() => settingsStore.moveShelf(i, i - 1)}
            class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)] disabled:opacity-30"
          >
            <WaveIcon name="chevronUp" size={15} />
          </button>
          <button
            type="button"
            disabled={i === settingsStore.homeShelves.length - 1}
            onclick={() => settingsStore.moveShelf(i, i + 1)}
            class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] hover:bg-[var(--lw-wash-hover)] disabled:opacity-30"
          >
            <WaveIcon name="chevronDown" size={15} />
          </button>
        </div>
      </div>
    {/each}
  </div>
</div>
