<script lang="ts">
  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();
  let dialogEl = $state<HTMLDialogElement | null>(null);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) dialogEl.showModal();
    else if (!open && dialogEl.open) dialogEl.close();
  });
</script>

<dialog bind:this={dialogEl} onclose={onClose} aria-labelledby="lastfm-guide-title" class="w-full max-w-[460px]">
  <div class="p-6">
    <h1 id="lastfm-guide-title" class="text-[18px] font-bold">Custom Last.fm API Keys</h1>
    <ol class="mt-3 list-decimal space-y-2 pl-5 text-[13px] text-[var(--lw-text-secondary)]">
      <li>Visit <span class="text-[var(--lw-text-primary)]">last.fm/api/account/create</span> and sign in.</li>
      <li>Create a desktop application entry to receive an API Key and Shared Secret.</li>
      <li>Paste both values in Settings → Scrobbler to use your own rate limit bucket.</li>
    </ol>
    <div class="mt-5 flex justify-end">
      <button
        type="button"
        onclick={onClose}
        class="h-9 cursor-pointer rounded-[10px] bg-[var(--lw-accent)] px-4 text-[13px] font-semibold text-[var(--lw-on-accent)]"
      >
        Got it
      </button>
    </div>
  </div>
</dialog>
