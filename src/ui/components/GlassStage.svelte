<script lang="ts">
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';

  let artwork = $derived(playerStore.currentTrack?.artworkUrl ?? '');
</script>

{#if settingsStore.glassmorphism && artwork}
  <div data-lw="glass-stage" class="pointer-events-none fixed inset-0 -z-10 overflow-hidden" aria-hidden="true">
    <img
      src={artwork}
      alt=""
      class="h-full w-full scale-125 object-cover opacity-35 blur-[72px] saturate-150"
    />
    <div class="absolute inset-0 bg-[var(--lw-bg-deep)]/60"></div>
  </div>
{/if}
