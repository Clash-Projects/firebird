<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';

  let dialogEl = $state<HTMLDialogElement | null>(null);

  $effect(() => {
    if (!dialogEl) return;
    if (windowState.updateDialogOpen && !dialogEl.open) dialogEl.showModal();
    else if (!windowState.updateDialogOpen && dialogEl.open) dialogEl.close();
  });

  function close() {
    windowState.updateDialogOpen = false;
  }
</script>

<dialog bind:this={dialogEl} onclose={close} aria-labelledby="update-title" class="w-full max-w-[420px]">
  <div class="p-6">
    <h1 id="update-title" class="text-[18px] font-bold">LastWave is up to date</h1>
    <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">
      You are running version <strong>2.6.0</strong> (Firebird Svelte 5 + Tailwind v4 UI).
    </p>
    <div class="mt-5 flex justify-end">
      <Buttons variant="primary" onclick={close}>Close</Buttons>
    </div>
  </div>
</dialog>
