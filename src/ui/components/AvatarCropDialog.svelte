<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import Buttons from './Buttons.svelte';
  import WaveSlider from './WaveSlider.svelte';

  interface Props {
    open: boolean;
    imageUrl: string;
    onSave: (zoom: number) => void;
    onClose: () => void;
  }

  let { open, imageUrl, onSave, onClose }: Props = $props();
  let dialogEl = $state<HTMLDialogElement | null>(null);
  let zoom = $state(1);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) dialogEl.showModal();
    else if (!open && dialogEl.open) dialogEl.close();
  });
</script>

<dialog
  bind:this={dialogEl}
  closedby="any"
  use:dismissDialogOnBackdrop={onClose}
  onclose={onClose}
  aria-labelledby="crop-title"
  class="w-full max-w-[400px]"
>
  <div class="p-6">
    <h1 id="crop-title" class="text-[17px] font-bold">Adjust Cover Crop</h1>
    <div class="mt-4 flex justify-center">
      <div class="h-48 w-48 overflow-hidden rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface)]">
        <img
          src={imageUrl}
          alt="Crop preview"
          class="h-full w-full object-cover"
          style:transform="scale({zoom})"
        />
      </div>
    </div>
    <div class="mt-4 flex items-center gap-3">
      <span class="text-[12px] text-[var(--lw-text-secondary)]">Zoom</span>
      <WaveSlider
        label="Cover zoom"
        min={1}
        max={2}
        step={0.05}
        bind:value={zoom}
        valuetext={`${Math.round(zoom * 100)}%`}
        class="flex-1"
      />
      <span class="w-10 text-right text-[12px] lw-tabular">{Math.round(zoom * 100)}%</span>
    </div>
    <div class="mt-5 flex justify-end gap-2">
      <Buttons variant="ghost" onclick={onClose}>Cancel</Buttons>
      <Buttons variant="primary" onclick={() => onSave(zoom)}>Apply</Buttons>
    </div>
  </div>
</dialog>
