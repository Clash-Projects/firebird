<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import Buttons from '$src/ui/components/Buttons.svelte';

  interface UpdateInfo {
    version: string;
    title?: string;
    changelog?: string;
    htmlUrl?: string;
  }

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let checking = $state(false);
  let update = $state<UpdateInfo | null>(null);

  $effect(() => {
    if (!dialogEl) return;
    if (windowState.updateDialogOpen && !dialogEl.open) {
      dialogEl.showModal();
      if (isTauriRuntime()) {
        checking = true;
        void ipcInvoke<UpdateInfo | null>('updates_check')
          .then((res) => {
            update = res;
          })
          .finally(() => {
            checking = false;
          });
      }
    } else if (!windowState.updateDialogOpen && dialogEl.open) {
      dialogEl.close();
    }
  });

  function close() {
    windowState.updateDialogOpen = false;
  }
</script>

<dialog bind:this={dialogEl} onclose={close} aria-labelledby="update-title" class="w-full max-w-[420px]">
  <div class="p-6">
    {#if checking}
      <h1 id="update-title" class="text-[18px] font-bold">Checking for updates...</h1>
      <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">Checking GitHub releases for a newer version.</p>
    {:else if update}
      <h1 id="update-title" class="text-[18px] font-bold">Update Available: v{update.version}</h1>
      <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">
        {update.title || 'A new version of LastWave is available.'}
      </p>
    {:else}
      <h1 id="update-title" class="text-[18px] font-bold">LastWave is up to date</h1>
      <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">
        You are running version <strong>2.6.0</strong>.
      </p>
    {/if}
    <div class="mt-5 flex justify-end gap-2">
      {#if update?.htmlUrl}
        <a
          href={update.htmlUrl}
          target="_blank"
          rel="noreferrer"
          class="inline-flex items-center rounded-[10px] bg-[var(--lw-accent)] px-4 py-2 text-[12.5px] font-semibold text-[var(--lw-on-accent)]"
        >
          View Release
        </a>
      {/if}
      <Buttons variant="secondary" onclick={close}>Close</Buttons>
    </div>
  </div>
</dialog>
