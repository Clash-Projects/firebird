<script lang="ts">
  import Buttons from '$src/ui/components/Buttons.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();
</script>

<div class="relative flex h-full w-full flex-col overflow-hidden bg-[var(--lw-bg)] select-none">
  <!-- Window drag chrome matching TitleBar -->
  <div class="h-10 shrink-0" data-tauri-drag-region></div>

  <!-- Subtle radial accent glow (matches OG welcome_page.dart) -->
  <div
    class="pointer-events-none absolute inset-x-0 top-0 h-[420px]"
    style:background="radial-gradient(ellipse 65% 75% at 50% 10%, color-mix(in srgb, var(--lw-accent) 14%, transparent), transparent 72%)"
    aria-hidden="true"
  ></div>

  <div class="relative z-10 flex flex-1 items-center justify-center p-6">
    <div class="w-full max-w-[440px] rounded-[16px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-8 text-center shadow-[var(--lw-shadow-artwork)]">
      <div class="mx-auto flex h-14 w-14 items-center justify-center rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-raised)]">
        <BrandIcon name="lastwave" size={28} />
      </div>

      <h1 class="mt-5 text-[22px] font-bold tracking-[-0.4px] text-[var(--lw-text-primary)]">
        LastWave Desktop
      </h1>
      <p class="mt-2 text-[13px] leading-[1.45] text-[var(--lw-text-secondary)]">
        Audiophile playback with lossless signal paths, 10-band EQ &amp; Clarity DSP, synced word-by-word lyrics, and plugins.
      </p>

      <div class="mt-6 grid grid-cols-3 gap-2 text-[11px] text-[var(--lw-text-secondary)]">
        <div class="rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-overlay)] px-2.5 py-2.5">
          <WaveIcon name="eq" size={15} class="mx-auto text-[var(--lw-accent)]" />
          <div class="mt-1 font-medium">Bit-Perfect</div>
        </div>
        <div class="rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-overlay)] px-2.5 py-2.5">
          <WaveIcon name="artist" size={15} class="mx-auto text-[var(--lw-accent)]" />
          <div class="mt-1 font-medium">Word Sync</div>
        </div>
        <div class="rounded-[10px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface-overlay)] px-2.5 py-2.5">
          <WaveIcon name="gauge" size={15} class="mx-auto text-[var(--lw-accent)]" />
          <div class="mt-1 font-medium">Studio DSP</div>
        </div>
      </div>

      <div class="mt-6 flex flex-col gap-2">
        <Buttons variant="primary" class="w-full" onclick={onComplete}>Open LastWave</Buttons>
        <Buttons variant="secondary" class="w-full" onclick={onComplete}>Continue as Guest</Buttons>
      </div>

      <div class="mt-5 flex justify-center">
        <a
          href="https://github.com/Clash-Projects/LastWave-Desktop"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex cursor-pointer items-center gap-1.5 text-[11.5px] text-[var(--lw-text-tertiary)] transition-colors hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="external" size={12} />
          <span>github.com/Clash-Projects/LastWave-Desktop</span>
        </a>
      </div>
    </div>
  </div>
</div>
