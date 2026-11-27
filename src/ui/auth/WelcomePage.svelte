<script lang="ts">
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();
</script>

<div class="flex h-full w-full items-center justify-center bg-[var(--lw-bg)] p-6">
  <div class="w-full max-w-[460px] rounded-[24px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-8 text-center shadow-[var(--lw-shadow-artwork)]">
    <div class="mx-auto flex h-14 w-14 items-center justify-center rounded-[16px] bg-[var(--lw-surface-raised)]">
      <BrandIcon name="lastwave" size={28} />
    </div>
    <h1 class="mt-5 text-[24px] font-bold tracking-[-0.4px]">Welcome to LastWave</h1>
    <p class="mt-2 text-[13px] text-[var(--lw-text-secondary)]">
      Audiophile desktop playback with Hi-Res lossless audio, synced word-level lyrics, and community plugins.
    </p>
    <div class="mt-6 flex flex-col gap-2.5">
      <Buttons variant="primary" class="w-full" onclick={onComplete}>Continue to Library</Buttons>
      <Buttons variant="secondary" class="w-full" onclick={onComplete}>Start in Guest Mode</Buttons>
    </div>
  </div>
</div>
