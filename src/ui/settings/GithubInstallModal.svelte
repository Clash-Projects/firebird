<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onInstalled?: () => void;
  }

  let { open, onClose, onInstalled }: Props = $props();
  let dialogEl = $state<HTMLDialogElement | null>(null);
  let repoInput = $state('');
  let installing = $state(false);
  let errorMsg = $state<string | null>(null);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      errorMsg = null;
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

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
</script>

<dialog
  bind:this={dialogEl}
  closedby="any"
  use:dismissDialogOnBackdrop={onClose}
  onclose={onClose}
  aria-labelledby="github-install-title"
  class="w-full max-w-[460px] p-5"
>
  <div class="flex items-start justify-between gap-3">
    <div>
      <div class="text-[10.5px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
        Direct Community Install
      </div>
      <h3 id="github-install-title" class="mt-0.5 text-[16px] font-bold tracking-[-0.02em]">
        Install from GitHub
      </h3>
    </div>
    <button
      type="button"
      onclick={onClose}
      use:waveTooltip={'Close'}
      aria-label="Close"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-tertiary)] transition-colors duration-[140ms] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  <p class="mt-2.5 text-[12.5px] leading-relaxed text-[var(--lw-text-secondary)]">
    Enter <code class="rounded bg-[var(--lw-wash)] px-1.5 py-0.5 text-[11.5px]">owner/repo</code>,
    <code class="rounded bg-[var(--lw-wash)] px-1.5 py-0.5 text-[11.5px]">owner/repo@commit</code>, or a raw
    <code class="rounded bg-[var(--lw-wash)] px-1.5 py-0.5 text-[11.5px]">manifest.json</code> URL. Repositories without a pinned commit resolve the latest SHA via the GitHub API.
  </p>

  <form onsubmit={handleSubmit} class="mt-4 space-y-3">
    <Input
      bind:value={repoInput}
      autofocus
      placeholder="owner/repo, owner/repo@commit, or manifest.json URL"
    />

    {#if errorMsg}
      <div class="rounded-[8px] border border-[var(--lw-danger)]/35 bg-[var(--lw-danger)]/12 px-3 py-2 text-[12px] text-[var(--lw-danger)]">
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
</dialog>
