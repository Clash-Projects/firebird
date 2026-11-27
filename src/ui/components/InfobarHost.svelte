<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { waveSlide } from '$src/ui/theme/motion';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  function toneColor(sev: string): string {
    if (sev === 'success') return 'text-[var(--lw-success)]';
    if (sev === 'warning') return 'text-[var(--lw-warn)]';
    if (sev === 'error') return 'text-[var(--lw-danger)]';
    return 'text-[var(--lw-accent)]';
  }
</script>

<div
  data-lw="infobar-host"
  class="pointer-events-none fixed right-5 bottom-[calc(var(--lw-dock-h)+14px)] z-50 flex flex-col gap-2"
>
  {#each featuresStore.toasts as toast (toast.id)}
    <div transition:waveSlide={{ y: 8 }}>
    <WaveHaze
      level="l3"
      class="pointer-events-auto flex min-w-[260px] max-w-[360px] items-center gap-3 rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3.5 py-2.5 shadow-[var(--lw-shadow-flyout)]"
    >
      <WaveIcon name={toast.severity === 'error' ? 'alert' : 'check'} size={16} class={toneColor(toast.severity)} />
      <div class="min-w-0 flex-1" role="status">
        <div class="truncate text-[12.5px] font-semibold text-[var(--lw-text-primary)]">{toast.title}</div>
        {#if toast.subtitle}
          <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{toast.subtitle}</div>
        {/if}
      </div>
      {#if toast.actionLabel && toast.onAction}
        <button
          type="button"
          onclick={() => {
            toast.onAction?.();
            featuresStore.dismissToast(toast.id);
          }}
          class="cursor-pointer rounded-[8px] bg-[var(--lw-wash-hover)] px-2 py-1 text-[11.5px] font-semibold hover:bg-[var(--lw-wash-pressed)]"
        >
          {toast.actionLabel}
        </button>
      {/if}
      <button
        type="button"
        onclick={() => featuresStore.dismissToast(toast.id)}
        aria-label="Dismiss notification"
        class="cursor-pointer rounded-[6px] p-1 text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name="close" size={13} />
      </button>
    </WaveHaze>
    </div>
  {/each}
</div>
