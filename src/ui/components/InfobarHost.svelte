<script lang="ts">
  import { featuresStore } from '$src/state/features.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSlide } from '$src/ui/theme/motion';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  function toneColor(sev: string): string {
    if (sev === 'success') return 'var(--lw-success)';
    if (sev === 'warning') return 'var(--lw-warn)';
    if (sev === 'error') return 'var(--lw-danger)';
    return 'var(--lw-accent)';
  }

  function iconName(sev: string) {
    if (sev === 'error' || sev === 'warning') return 'alert' as const;
    if (sev === 'success') return 'check' as const;
    return 'info' as const;
  }
</script>

<div
  data-lw="infobar-host"
  class="pointer-events-none fixed right-4 bottom-[calc(var(--lw-dock-h)+12px)] z-50 flex flex-col items-end gap-2"
>
  {#each featuresStore.toasts as toast (toast.id)}
    {@const tone = toneColor(toast.severity)}
    <div transition:waveSlide={{ y: 8 }}>
      <WaveHaze
        level="l3"
        class="pointer-events-auto relative flex min-w-[270px] max-w-[380px] items-center gap-3 overflow-hidden rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] px-3.5 py-2.5 shadow-[var(--lw-shadow-flyout)]"
      >
        <span
          class="absolute inset-y-0 left-0 w-[3px]"
          style:background={tone}
          aria-hidden="true"
        ></span>
        <span class="flex shrink-0" style:color={tone}>
          <WaveIcon name={iconName(toast.severity)} size={15} />
        </span>
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
            class="cursor-pointer rounded-[6px] bg-[var(--lw-wash-hover)] px-2.5 py-1 text-[11.5px] font-medium text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-pressed)]"
          >
            {toast.actionLabel}
          </button>
        {/if}
        <button
          type="button"
          use:waveTooltip={'Dismiss'}
          onclick={() => featuresStore.dismissToast(toast.id)}
          aria-label="Dismiss notification"
          class="cursor-pointer rounded-[6px] p-1 text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="close" size={12} />
        </button>
      </WaveHaze>
    </div>
  {/each}
</div>
