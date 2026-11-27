<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
</script>

<label class="flex cursor-pointer items-center justify-between gap-4 rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Performance Mode</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">
      Disables backdrop blur, ambient stages, and stagger animations for minimal GPU usage.
    </p>
  </div>
  <input type="checkbox" bind:checked={settingsStore.performanceMode} class="h-4 w-4 accent-[var(--lw-accent)]" />
</label>
