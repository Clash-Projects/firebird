<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
  import { NOW_PLAYING_STYLES } from '$src/ui/now_playing/nowPlayingStyle';
</script>

<div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
  {#each NOW_PLAYING_STYLES as style (style.id)}
    {@const active = settingsStore.nowPlayingStyle === style.id}
    <button
      type="button"
      onclick={() => (settingsStore.nowPlayingStyle = style.id)}
      class="cursor-pointer rounded-[14px] border p-3.5 text-left transition-colors {active
        ? 'border-[var(--lw-accent)] bg-[var(--lw-wash-pressed)]'
        : 'border-[var(--lw-outline)] bg-[var(--lw-surface)] hover:bg-[var(--lw-wash-hover)]'}"
    >
      <div class="text-[13px] font-semibold">{style.label}</div>
      <p class="mt-1 text-[11.5px] text-[var(--lw-text-secondary)]">{style.description}</p>
    </button>
  {/each}
</div>
