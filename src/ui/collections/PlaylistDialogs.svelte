<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onCreated?: (id: string) => void;
  }

  let { open, onClose, onCreated }: Props = $props();
  let dialogEl = $state<HTMLDialogElement | null>(null);
  let name = $state('');
  let description = $state('');
  let creating = $state(false);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      name = '';
      description = '';
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (creating) return;
    creating = true;
    const pl = await libraryStore.createPlaylist(name, description);
    creating = false;
    if (!pl) {
      featuresStore.showToast({ title: 'Could not create playlist', severity: 'warning' });
      return;
    }
    if (!open) return;
    onCreated?.(pl.id);
    onClose();
  }
</script>

<dialog
  bind:this={dialogEl}
  closedby="any"
  use:dismissDialogOnBackdrop={onClose}
  onclose={onClose}
  aria-labelledby="new-pl-title"
  class="w-full max-w-[420px]"
>
  <form onsubmit={submit} class="p-6">
    <h1 id="new-pl-title" class="text-[18px] font-bold">New Playlist</h1>
    <div class="mt-4 space-y-3">
      <Input bind:value={name} placeholder="Playlist name" autofocus />
      <Input bind:value={description} placeholder="Description (optional)" />
    </div>
    <div class="mt-5 flex justify-end gap-2">
      <Buttons variant="ghost" onclick={onClose}>Cancel</Buttons>
      <Buttons type="submit" variant="primary" disabled={creating}>Create</Buttons>
    </div>
  </form>
</dialog>
