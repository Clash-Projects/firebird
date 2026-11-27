<script lang="ts">
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onInstalled?: () => void;
  }

  let { open, onClose, onInstalled }: Props = $props();
  let repoInput = $state('');
  let installing = $state(false);
  let errorMsg = $state<string | null>(null);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!repoInput.trim() || installing) return;
    installing = true;
    errorMsg = null;
    try {
      await pluginManager.installFromUrl(repoInput.trim());
      repoInput = '';
      onInstalled?.();
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Could not load manifest.json from GitHub';
      errorMsg = msg;
      featuresStore.showToast({
        title: 'GitHub install failed',
        subtitle: msg,
        severity: 'error',
      });
    } finally {
      installing = false;
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-4"
    role="dialog"
    aria-modal="true"
    aria-label="Install plugin from GitHub"
  >
    <button
      type="button"
      aria-label="Close modal"
      onclick={onClose}
      class="absolute inset-0 cursor-default"
    ></button>

    <div
      class="relative z-10 w-full max-w-[480px] rounded-[16px] border border-[rgba(255,255,255,0.08)] border-t-[rgba(255,255,255,0.14)] border-b-[rgba(0,0,0,0.5)] bg-[var(--lw-bg-elevated,var(--lw-surface-raised))] p-5 shadow-[0_24px_48px_rgba(0,0,0,0.55)]"
    >
      <div class="flex items-start justify-between gap-3">
        <div>
          <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
            Direct Community Install
          </div>
          <h3 class="mt-0.5 text-[17px] font-bold tracking-[-0.02em]">Install from GitHub</h3>
        </div>
        <button
          type="button"
          onclick={onClose}
          title="Close"
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-tertiary)] transition-colors duration-[160ms] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="close" size={16} />
        </button>
      </div>

      <p class="mt-2.5 text-[12.5px] leading-relaxed text-[var(--lw-text-secondary)]">
        Enter <code class="rounded bg-white/8 px-1.5 py-0.5 text-[11.5px]">owner/repo</code>,
        <code class="rounded bg-white/8 px-1.5 py-0.5 text-[11.5px]">owner/repo@commit</code>, or a raw
        <code class="rounded bg-white/8 px-1.5 py-0.5 text-[11.5px]">manifest.json</code> URL. Repositories without a pinned commit resolve the latest SHA via the GitHub API.
      </p>

      <form onsubmit={handleSubmit} class="mt-4 space-y-3">
        <Input
          bind:value={repoInput}
          placeholder="e.g. username/lastwave-midnight or owner/repo@a1b2c3d"
        />

        {#if errorMsg}
          <div class="rounded-[10px] border border-[var(--lw-danger)]/35 bg-[var(--lw-danger)]/12 px-3 py-2 text-[12px] text-[var(--lw-danger)]">
            {errorMsg}
          </div>
        {/if}

        <div class="flex items-center justify-end gap-2 pt-1">
          <Buttons variant="ghost" size="sm" onclick={onClose} disabled={installing}>
            Cancel
          </Buttons>
          <Buttons type="submit" variant="primary" size="sm" disabled={installing || !repoInput.trim()}>
            {installing ? 'Resolving & Installing...' : 'Install Plugin'}
          </Buttons>
        </div>
      </form>
    </div>
  </div>
{/if}
