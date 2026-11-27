<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
</script>

<div class="flex flex-wrap items-center justify-between gap-3 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
  <div>
    <div class="text-[13.5px] font-semibold">Diagnostics Bundle</div>
    <p class="text-[12px] text-[var(--lw-text-secondary)]">Redacts tokens and cookies automatically before exporting logs.</p>
  </div>
  <Buttons
    variant="secondary"
    size="sm"
    onclick={() => featuresStore.showToast({ title: 'Diagnostics bundle exported', subtitle: 'Redacted log zip ready', severity: 'success' })}
  >
    Export Logs
  </Buttons>
</div>
