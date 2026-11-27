<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import type { PluginType } from '$src/lib/plugins/types';
  import { featuresStore } from '$src/state/features.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onInstalled?: () => void;
  }

  let { open, onClose, onInstalled }: Props = $props();

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let folderInput: HTMLInputElement | undefined = $state();
  let filesInput: HTMLInputElement | undefined = $state();
  let selectedFiles = $state<File[]>([]);
  let customName = $state('');
  let customType = $state<PluginType>('theme');
  let installing = $state(false);
  let errorMsg = $state<string | null>(null);

  let hasManifest = $derived(
    selectedFiles.some((f) => f.name.toLowerCase() === 'manifest.json')
  );

  function resetState() {
    selectedFiles = [];
    customName = '';
    customType = 'theme';
    errorMsg = null;
  }

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      resetState();
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  function handlePicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const picked = Array.from(input.files ?? []);
    input.value = '';
    if (picked.length === 0) return;

    selectedFiles = picked;
    errorMsg = null;

    const first =
      picked.find((f) => /\.(css|json|js)$/i.test(f.name) && f.name.toLowerCase() !== 'manifest.json') ??
      picked[0];
    if (first && !hasManifest) {
      const ext = first.name.split('.').pop()?.toLowerCase() || '';
      customType = ext === 'js' ? 'widget' : ext === 'json' ? 'palette' : 'theme';
      customName = first.name.replace(/\.[^.]+$/, '').replace(/[-_]+/g, ' ');
    }
  }

  async function installSelected() {
    if (selectedFiles.length === 0 || installing) return;
    installing = true;
    errorMsg = null;
    try {
      await pluginManager.installFromLocalFiles(
        selectedFiles,
        hasManifest ? undefined : { name: customName, type: customType }
      );
      resetState();
      onInstalled?.();
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to install local plugin';
      errorMsg = msg;
      featuresStore.showToast({
        title: 'Local install failed',
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
  aria-labelledby="local-install-title"
  class="w-full max-w-[460px] p-5"
>
  <div class="flex items-start justify-between gap-3">
    <div>
      <div class="text-[10.5px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
        Local Disk
      </div>
      <h3 id="local-install-title" class="mt-0.5 text-[16px] font-bold tracking-[-0.02em]">
        Install Local Plugin
      </h3>
    </div>
    <button
      type="button"
      onclick={onClose}
      use:waveTooltip={'Close'}
      aria-label="Close"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-tertiary)] transition-colors hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  <p class="mt-2.5 text-[12.5px] leading-relaxed text-[var(--lw-text-secondary)]">
    Pick a local plugin folder (containing <code class="font-mono text-[11.5px]">manifest.json</code> + entry file)
    or load a standalone <code class="font-mono text-[11.5px]">.json</code>, <code class="font-mono text-[11.5px]">.css</code>, or <code class="font-mono text-[11.5px]">.js</code> file directly.
  </p>

  <input
    bind:this={folderInput}
    type="file"
    webkitdirectory
    multiple
    class="hidden"
    onchange={handlePicked}
  />
  <input
    bind:this={filesInput}
    type="file"
    multiple
    accept=".json,.css,.js"
    class="hidden"
    onchange={handlePicked}
  />

  <div class="mt-4 flex flex-wrap gap-2">
    <Buttons variant="secondary" size="sm" onclick={() => folderInput?.click()}>
      <WaveIcon name="folderOpen" size={14} />
      Choose Folder
    </Buttons>
    <Buttons variant="secondary" size="sm" onclick={() => filesInput?.click()}>
      <WaveIcon name="upload" size={14} />
      Choose File(s)
    </Buttons>
  </div>

  {#if selectedFiles.length > 0}
    <div class="mt-3.5 space-y-3 rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3">
      <div class="text-[12px] text-[var(--lw-text-secondary)]">
        Selected: <strong class="text-[var(--lw-text-primary)]">{selectedFiles.length} file{selectedFiles.length === 1 ? '' : 's'}</strong>
        <span class="ml-1.5 font-mono text-[11px] text-[var(--lw-text-tertiary)]">
          ({selectedFiles.slice(0, 3).map((f) => f.name).join(', ')}{selectedFiles.length > 3 ? '…' : ''})
        </span>
      </div>

      {#if !hasManifest}
        <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          <div>
            <div class="mb-1 text-[11px] font-medium text-[var(--lw-text-tertiary)]">Plugin Name</div>
            <Input bind:value={customName} placeholder="My Custom Theme" />
          </div>
          <div>
            <div class="mb-1 text-[11px] font-medium text-[var(--lw-text-tertiary)]">Type</div>
            <Select
              label="Plugin type"
              bind:value={customType}
              options={[
                { value: 'palette', label: 'Palette (JSON)' },
                { value: 'theme', label: 'Theme (CSS)' },
                { value: 'snippet', label: 'Snippet (CSS)' },
                { value: 'widget', label: 'Widget (JS)' },
              ]}
              class="w-full"
            />
          </div>
        </div>
      {/if}
    </div>
  {/if}

  {#if errorMsg}
    <div class="mt-3 rounded-[8px] bg-[var(--lw-danger)]/14 px-3 py-2 text-[12px] text-[var(--lw-danger)]">
      {errorMsg}
    </div>
  {/if}

  <div class="mt-4 flex justify-end gap-2">
    <Buttons variant="ghost" size="sm" onclick={onClose}>Cancel</Buttons>
    <Buttons
      variant="primary"
      size="sm"
      disabled={selectedFiles.length === 0 || installing}
      onclick={() => void installSelected()}
    >
      {installing ? 'Installing...' : 'Install & Activate'}
    </Buttons>
  </div>
</dialog>
