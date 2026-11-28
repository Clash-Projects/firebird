<script lang="ts">
  import { flip } from 'svelte/animate';
  import { cubicOut } from 'svelte/easing';
  import { prefersReducedMotion } from 'svelte/motion';
  import { featuresStore } from '$src/state/features.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { ToastMessage } from '$src/state/types';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSlide } from '$src/ui/theme/motion';
  import { WaveMotion } from '$src/ui/theme/tokens';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  const appearance: Record<ToastMessage['severity'], { tone: string; icon: WaveIconName }> = {
    success: { tone: 'var(--lw-text-primary)', icon: 'check' },
    warning: { tone: 'var(--lw-warn)', icon: 'alert' },
    error: { tone: 'var(--lw-danger)', icon: 'error' },
    info: { tone: 'var(--lw-text-secondary)', icon: 'info' },
  };

  let stackDuration = $derived(
    settingsStore.reduceMotion || prefersReducedMotion.current ? 0 : WaveMotion.normalMs,
  );
</script>

<div data-lw="infobar-host" class="notification-stack">
  {#each featuresStore.toasts as toast (toast.id)}
    {@const look = appearance[toast.severity] ?? appearance.info}
    <div class="notification-slot" animate:flip={{ duration: stackDuration, easing: cubicOut }}>
      <div
        class="notification"
        style:--notification-tone={look.tone}
        transition:waveSlide={{ y: 6 }}
      >
        <div class="notification-icon" aria-hidden="true">
          <WaveIcon name={look.icon} size={16} />
        </div>

        <div class="notification-content">
          <div role="status" aria-atomic="true">
            <div class="notification-title">{toast.title}</div>
            {#if toast.subtitle}
              <div class="notification-detail">{toast.subtitle}</div>
            {/if}
          </div>
          {#if toast.actionLabel && toast.onAction}
            <button
              type="button"
              class="notification-action lw-press"
              onclick={() => {
                toast.onAction?.();
                featuresStore.dismissToast(toast.id);
              }}
            >
              {toast.actionLabel}
            </button>
          {/if}
        </div>

        <button
          type="button"
          class="notification-dismiss lw-press"
          use:waveTooltip={'Dismiss'}
          onclick={() => featuresStore.dismissToast(toast.id)}
          aria-label="Dismiss notification"
        >
          <WaveIcon name="close" size={12} />
        </button>
      </div>
    </div>
  {/each}
</div>

<style>
  .notification-stack {
    position: fixed;
    z-index: 50;
    inset-inline-end: 14px;
    bottom: calc(var(--lw-dock-h) + 14px);
    display: flex;
    width: min(320px, calc(100% - 28px));
    flex-direction: column;
    gap: 6px;
    pointer-events: none;
  }

  .notification-slot {
    width: 100%;
  }

  .notification {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) 26px;
    align-items: center;
    gap: 10px;
    padding: 9px 8px 9px 12px;
    border: 1px solid var(--lw-outline);
    border-radius: 8px;
    background: var(--lw-surface-raised);
    box-shadow: var(--lw-shadow-flyout);
    pointer-events: auto;
  }

  .notification-icon {
    display: flex;
    color: color-mix(in srgb, var(--notification-tone) 72%, var(--lw-text-primary));
  }

  .notification-content {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .notification-title {
    color: var(--lw-text-primary);
    font-size: 13px;
    font-weight: 600;
    line-height: 1.35;
  }

  .notification-detail {
    display: -webkit-box;
    margin-top: 2px;
    overflow: hidden;
    color: var(--lw-text-secondary);
    font-size: 12px;
    line-height: 1.4;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .notification-action,
  .notification-dismiss {
    cursor: pointer;
    transition: background-color var(--lw-motion-fast) var(--ease-wave),
      color var(--lw-motion-fast) var(--ease-wave);
  }

  .notification-action {
    max-width: 100%;
    margin-top: 5px;
    border-radius: 3px;
    color: var(--lw-text-primary);
    font-size: 12px;
    font-weight: 600;
    text-align: start;
    text-decoration: underline;
    text-decoration-color: var(--lw-text-tertiary);
    text-underline-offset: 2px;
  }

  .notification-dismiss {
    display: grid;
    width: 26px;
    height: 26px;
    place-items: center;
    border-radius: 5px;
    color: var(--lw-text-secondary);
  }

  @media (hover: hover) and (pointer: fine) {
    .notification-action:hover {
      text-decoration-color: currentColor;
    }

    .notification-dismiss:hover {
      background: var(--lw-wash-hover);
      color: var(--lw-text-primary);
    }
  }
</style>
