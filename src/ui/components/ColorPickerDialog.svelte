<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import Buttons from './Buttons.svelte';
  import Input from './Input.svelte';

  interface Props {
    open: boolean;
    title: string;
    initialColor: string;
    onClose: () => void;
    onApply: (hex: string) => void;
  }

  let { open, title, initialColor, onClose, onApply }: Props = $props();

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let svBoxEl = $state<HTMLDivElement | null>(null);
  let hueBarEl = $state<HTMLDivElement | null>(null);

  let hue = $state(0);
  let sat = $state(1);
  let val = $state(1);
  let hexDraft = $state('#F5F4F0');
  let draggingSv = $state(false);
  let draggingHue = $state(false);

  function hexToHsv(hex: string): [number, number, number] {
    const clean = hex.replace('#', '').trim();
    const full =
      clean.length === 3
        ? clean
            .split('')
            .map((c) => c + c)
            .join('')
        : clean.padEnd(6, '0').slice(0, 6);
    const num = Number.parseInt(full, 16);
    if (!Number.isFinite(num)) return [0, 0, 0.96];
    const r = ((num >> 16) & 255) / 255;
    const g = ((num >> 8) & 255) / 255;
    const b = (num & 255) / 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const d = max - min;
    let h = 0;
    if (d !== 0) {
      if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) * 60;
      else if (max === g) h = ((b - r) / d + 2) * 60;
      else h = ((r - g) / d + 4) * 60;
    }
    const s = max === 0 ? 0 : d / max;
    return [h, s, max];
  }

  function hsvToHex(h: number, s: number, v: number): string {
    const c = v * s;
    const hh = ((h % 360) + 360) % 360;
    const x = c * (1 - Math.abs(((hh / 60) % 2) - 1));
    const m = v - c;
    let r = 0,
      g = 0,
      b = 0;
    if (hh < 60) [r, g, b] = [c, x, 0];
    else if (hh < 120) [r, g, b] = [x, c, 0];
    else if (hh < 180) [r, g, b] = [0, c, x];
    else if (hh < 240) [r, g, b] = [0, x, c];
    else if (hh < 300) [r, g, b] = [x, 0, c];
    else [r, g, b] = [c, 0, x];
    const toByte = (n: number) =>
      Math.max(0, Math.min(255, Math.round((n + m) * 255)))
        .toString(16)
        .padStart(2, '0');
    return `#${toByte(r)}${toByte(g)}${toByte(b)}`.toUpperCase();
  }

  let currentHex = $derived(hsvToHex(hue, sat, val));
  let pureHueHex = $derived(hsvToHex(hue, 1, 1));

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      const [h, s, v] = hexToHsv(initialColor || '#F5F4F0');
      hue = h;
      sat = s;
      val = v;
      hexDraft = hsvToHex(h, s, v);
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  function updateFromSvPointer(e: PointerEvent) {
    if (!svBoxEl) return;
    const rect = svBoxEl.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    sat = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    val = Math.max(0, Math.min(1, 1 - (e.clientY - rect.top) / rect.height));
    hexDraft = hsvToHex(hue, sat, val);
  }

  function updateFromHuePointer(e: PointerEvent) {
    if (!hueBarEl) return;
    const rect = hueBarEl.getBoundingClientRect();
    if (rect.width <= 0) return;
    hue = Math.max(0, Math.min(360, ((e.clientX - rect.left) / rect.width) * 360));
    hexDraft = hsvToHex(hue, sat, val);
  }

  function onHexInput(raw: string) {
    hexDraft = raw;
    const trimmed = raw.trim().replace(/^#/, '');
    if (/^[0-9a-fA-F]{6}$/.test(trimmed)) {
      const [h, s, v] = hexToHsv(`#${trimmed}`);
      hue = h;
      sat = s;
      val = v;
    }
  }

  function submit() {
    onApply(currentHex);
    onClose();
  }
</script>

<svelte:window
  onpointermove={(e) => {
    if (draggingSv) updateFromSvPointer(e);
    else if (draggingHue) updateFromHuePointer(e);
  }}
  onpointerup={() => {
    draggingSv = false;
    draggingHue = false;
  }}
/>

<dialog
  bind:this={dialogEl}
  closedby="any"
  use:dismissDialogOnBackdrop={onClose}
  onclose={onClose}
  aria-label={title}
  class="w-[min(360px,calc(100vw-32px))] p-5 text-[var(--lw-text-primary)]"
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
    class="space-y-4"
  >
    <h2 class="text-[16px] font-semibold tracking-[-0.2px]">{title}</h2>

    <!-- 2D Saturation / Value spectrum -->
    <div
      bind:this={svBoxEl}
      role="slider"
      tabindex="0"
      aria-label="Colour saturation and brightness"
      aria-valuenow={Math.round(sat * 100)}
      onpointerdown={(e) => {
        e.preventDefault();
        draggingSv = true;
        updateFromSvPointer(e);
      }}
      class="relative h-[168px] w-full cursor-crosshair overflow-hidden rounded-[12px] border border-[var(--lw-outline)] select-none"
      style:background-color={pureHueHex}
      style:background-image="linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent)"
    >
      <span
        aria-hidden="true"
        class="pointer-events-none absolute h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_1px_4px_rgba(0,0,0,0.65)]"
        style:left="{sat * 100}%"
        style:top="{(1 - val) * 100}%"
        style:background-color={currentHex}
      ></span>
    </div>

    <!-- Hue bar -->
    <div
      bind:this={hueBarEl}
      role="slider"
      tabindex="0"
      aria-label="Hue"
      aria-valuemin={0}
      aria-valuemax={360}
      aria-valuenow={Math.round(hue)}
      onpointerdown={(e) => {
        e.preventDefault();
        draggingHue = true;
        updateFromHuePointer(e);
      }}
      class="relative h-3.5 w-full cursor-pointer rounded-full border border-[var(--lw-outline)] select-none"
      style:background="linear-gradient(to right, #ff0000 0%, #ffff00 17%, #00ff00 33%, #00ffff 50%, #0000ff 67%, #ff00ff 83%, #ff0000 100%)"
    >
      <span
        aria-hidden="true"
        class="pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_1px_4px_rgba(0,0,0,0.65)]"
        style:left="{(hue / 360) * 100}%"
        style:background-color={pureHueHex}
      ></span>
    </div>

    <!-- Preview + Hex input -->
    <div class="flex items-center gap-2.5">
      <div
        class="h-9 w-11 shrink-0 rounded-[10px] border border-[var(--lw-outline)]"
        style:background-color={currentHex}
      ></div>
      <Input
        value={hexDraft}
        oninput={onHexInput}
        ariaLabel="Hex colour code"
        placeholder="#F5F4F0"
        class="font-mono uppercase"
      />
    </div>

    <div class="flex items-center justify-end gap-2 pt-1">
      <Buttons variant="secondary" size="sm" onclick={onClose}>Cancel</Buttons>
      <Buttons variant="primary" size="sm" type="submit">Apply</Buttons>
    </div>
  </form>
</dialog>
