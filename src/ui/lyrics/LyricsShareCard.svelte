<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';

  interface CardStyle {
    label: string;
    top: string;
    bottom: string;
    text: string;
    subtext: string;
  }

  const PRESET_STYLES: ReadonlyArray<CardStyle> = [
    { label: 'Sunset', top: '#FF7A59', bottom: '#9B1D5A', text: '#FFFFFF', subtext: 'rgba(255,255,255,0.80)' },
    { label: 'Ocean', top: '#2B7DE9', bottom: '#0B2F6B', text: '#FFFFFF', subtext: 'rgba(255,255,255,0.80)' },
    { label: 'Forest', top: '#2E9E6B', bottom: '#0C3B2E', text: '#FFFFFF', subtext: 'rgba(255,255,255,0.80)' },
    { label: 'Violet', top: '#8E5CF0', bottom: '#2E1A6B', text: '#FFFFFF', subtext: 'rgba(255,255,255,0.80)' },
    { label: 'Paper', top: '#FAF6EE', bottom: '#E6DFD0', text: '#1C1B19', subtext: 'rgba(28,27,25,0.70)' },
    { label: 'Black', top: '#1E1E1E', bottom: '#000000', text: '#FFFFFF', subtext: 'rgba(255,255,255,0.80)' },
  ];

  interface Props {
    open: boolean;
    lines?: string[];
    lyricText?: string;
    onClose: () => void;
  }

  let { open, lines = [], lyricText = '', onClose }: Props = $props();

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let track = $derived(playerStore.currentTrack);
  let shownLines = $derived.by(() => {
    const src = lines.length > 0 ? lines : lyricText ? [lyricText] : [];
    return src.map((l) => l.trim()).filter(Boolean).slice(0, 10);
  });

  let artworkStyle = $state<CardStyle | null>(null);
  let loadedCoverImage = $state<HTMLImageElement | null>(null);
  let styles = $derived<ReadonlyArray<CardStyle>>(
    artworkStyle ? [artworkStyle, ...PRESET_STYLES] : PRESET_STYLES
  );
  let selectedIdx = $state(0);
  let selectedStyle = $derived(styles[Math.min(selectedIdx, styles.length - 1)] ?? PRESET_STYLES[0]);
  let saving = $state(false);

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      selectedIdx = 0;
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
    r /= 255;
    g /= 255;
    b /= 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    let h = 0;
    let s = 0;
    const l = (max + min) / 2;
    if (max !== min) {
      const d = max - min;
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
      switch (max) {
        case r:
          h = (g - b) / d + (g < b ? 6 : 0);
          break;
        case g:
          h = (b - r) / d + 2;
          break;
        default:
          h = (r - g) / d + 4;
          break;
      }
      h /= 6;
    }
    return [h * 360, s, l];
  }

  function hslHex(h: number, s: number, l: number): string {
    const c = (1 - Math.abs(2 * l - 1)) * s;
    const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
    const m = l - c / 2;
    let r = 0,
      g = 0,
      b = 0;
    if (h < 60) [r, g, b] = [c, x, 0];
    else if (h < 120) [r, g, b] = [x, c, 0];
    else if (h < 180) [r, g, b] = [0, c, x];
    else if (h < 240) [r, g, b] = [0, x, c];
    else if (h < 300) [r, g, b] = [x, 0, c];
    else [r, g, b] = [c, 0, x];
    const toHex = (v: number) =>
      Math.round((v + m) * 255)
        .toString(16)
        .padStart(2, '0');
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  }

  // Extract palette gradient from cover artwork (matches `_artworkStyle` in `lyrics_share_card.dart`)
  $effect(() => {
    if (!open || !track?.artworkUrl) {
      artworkStyle = null;
      loadedCoverImage = null;
      return;
    }
    let cancelled = false;
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      if (cancelled) return;
      loadedCoverImage = img;
      try {
        const c = document.createElement('canvas');
        c.width = 48;
        c.height = 48;
        const ctx = c.getContext('2d');
        if (!ctx) return;
        ctx.drawImage(img, 0, 0, 48, 48);
        const data = ctx.getImageData(0, 0, 48, 48).data;
        let bestH = 260,
          bestS = 0.5,
          bestL = 0.35,
          bestWeight = -1;
        for (let i = 0; i < data.length; i += 16) {
          if (data[i + 3] < 128) continue;
          const [h, s, l] = rgbToHsl(data[i], data[i + 1], data[i + 2]);
          const weight = s * (1 - Math.abs(l - 0.5));
          if (weight > bestWeight) {
            bestWeight = weight;
            bestH = h;
            bestS = s;
            bestL = l;
          }
        }
        const top = hslHex(bestH, Math.min(0.85, Math.max(0.3, bestS)), Math.min(0.42, Math.max(0.28, bestL)));
        const bottom = hslHex(
          bestH,
          Math.min(0.7, Math.max(0.25, bestS)),
          Math.min(0.18, Math.max(0.08, bestL * 0.35))
        );
        artworkStyle = {
          label: 'Artwork',
          top,
          bottom,
          text: '#FFFFFF',
          subtext: 'rgba(255,255,255,0.80)',
        };
      } catch {
        // Fallback to track accent color if CORS blocks pixel read
        if (track?.accentColor) {
          artworkStyle = {
            label: 'Artwork',
            top: track.accentColor,
            bottom: '#12101B',
            text: '#FFFFFF',
            subtext: 'rgba(255,255,255,0.80)',
          };
        }
      }
    };
    img.onerror = () => {
      if (cancelled) return;
      loadedCoverImage = null;
      if (track?.accentColor) {
        artworkStyle = {
          label: 'Artwork',
          top: track.accentColor,
          bottom: '#12101B',
          text: '#FFFFFF',
          subtext: 'rgba(255,255,255,0.80)',
        };
      }
    };
    img.src = track.artworkUrl;
    return () => {
      cancelled = true;
    };
  });

  function wrapCanvasLines(
    ctx: CanvasRenderingContext2D,
    text: string,
    maxWidth: number,
    maxLines: number
  ): string[] {
    const words = text.split(/\s+/).filter(Boolean);
    const out: string[] = [];
    let current = '';
    for (const word of words) {
      const candidate = current ? `${current} ${word}` : word;
      if (ctx.measureText(candidate).width <= maxWidth || !current) {
        current = candidate;
      } else {
        out.push(current);
        current = word;
        if (out.length >= maxLines - 1) break;
      }
    }
    if (current && out.length < maxLines) {
      while (ctx.measureText(current).width > maxWidth && current.length > 3) {
        current = `${current.slice(0, -2)}…`;
      }
      out.push(current);
    }
    return out;
  }

  /** Renders an exact 1080×1080 (`540 * 2`) PNG matching `_LyricsShareCard` in `lyrics_share_card.dart`. */
  async function renderCardBlob(): Promise<Blob | null> {
    const scale = 2;
    const size = 540 * scale;
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;

    ctx.scale(scale, scale);

    // 1. Gradient background
    const grad = ctx.createLinearGradient(0, 0, 540, 540);
    grad.addColorStop(0, selectedStyle.top);
    grad.addColorStop(1, selectedStyle.bottom);
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 540, 540);

    // 2. Cover art (112×112, radius 12) at (36, 36)
    ctx.save();
    ctx.beginPath();
    ctx.roundRect(36, 36, 112, 112, 12);
    ctx.clip();
    if (loadedCoverImage) {
      try {
        ctx.drawImage(loadedCoverImage, 36, 36, 112, 112);
      } catch {
        ctx.fillStyle = 'rgba(0,0,0,0.22)';
        ctx.fillRect(36, 36, 112, 112);
      }
    } else {
      ctx.fillStyle = 'rgba(0,0,0,0.22)';
      ctx.fillRect(36, 36, 112, 112);
    }
    ctx.restore();

    // 3. Title + Artist next to cover
    const fontStack = getComputedStyle(document.documentElement).getPropertyValue('--lw-font-family') || 'Segoe UI, sans-serif';
    const metaX = 36 + 112 + 18;
    const metaW = 540 - 36 - metaX;

    ctx.fillStyle = selectedStyle.text;
    ctx.font = `700 22px ${fontStack}`;
    ctx.textBaseline = 'top';
    const titleWrapped = wrapCanvasLines(ctx, track?.title || 'Unknown title', metaW, 2);
    let metaY = 54;
    for (const tl of titleWrapped) {
      ctx.fillText(tl, metaX, metaY);
      metaY += 26;
    }

    metaY += 4;
    ctx.fillStyle = selectedStyle.subtext;
    ctx.font = `500 15px ${fontStack}`;
    const artistWrapped = wrapCanvasLines(ctx, track?.artist || '', metaW, 2);
    for (const al of artistWrapped) {
      ctx.fillText(al, metaX, metaY);
      metaY += 19;
    }

    // 4. Lyric Lines (start at y = 36 + 112 + 28 = 176)
    ctx.fillStyle = selectedStyle.text;
    ctx.font = `600 18px ${fontStack}`;
    let lineY = 176;
    const bodyW = 540 - 72;
    for (const lyric of shownLines) {
      if (lineY > 540 - 44) break;
      const wrapped = wrapCanvasLines(ctx, lyric, bodyW, 2);
      for (const wl of wrapped) {
        if (lineY > 540 - 40) break;
        ctx.fillText(wl, 36, lineY);
        lineY += 23;
      }
      lineY += 8;
    }

    return new Promise((resolve) => canvas.toBlob((b) => resolve(b), 'image/png'));
  }

  async function saveCardPng() {
    if (saving) return;
    saving = true;
    try {
      const blob = await renderCardBlob();
      if (!blob) {
        featuresStore.showToast({ title: "Couldn't render the lyrics card", severity: 'error' });
        return;
      }
      const safe = (track?.title || 'lyrics').replace(/[\\/:*?"<>|]/g, '_').trim() || 'lyrics';
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${safe}.png`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 2000);
      featuresStore.showToast({ title: 'Lyrics card saved', severity: 'success' });
      onClose();
    } finally {
      saving = false;
    }
  }

  function copyQuote() {
    const body = shownLines.join('\n');
    const quote = `"${body}" — ${track?.artist ?? ''} (${track?.title ?? ''})`;
    navigator.clipboard?.writeText(quote).catch(() => {});
    featuresStore.showToast({ title: 'Lyrics copied', subtitle: track?.title, severity: 'success' });
    onClose();
  }
</script>

<dialog
  bind:this={dialogEl}
  closedby="any"
  use:dismissDialogOnBackdrop={onClose}
  onclose={onClose}
  aria-labelledby="share-lyric-title"
  class="w-[min(520px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <h2 id="share-lyric-title" class="text-[18px] font-semibold">Lyrics card</h2>

  <!-- Live 1:1 Square Card Preview -->
  <div
    class="mt-4 aspect-square w-full overflow-hidden rounded-[10px] p-7 shadow-[var(--lw-shadow-flyout)]"
    style:background={`linear-gradient(135deg, ${selectedStyle.top}, ${selectedStyle.bottom})`}
    style:color={selectedStyle.text}
  >
    <div class="flex h-full flex-col">
      <div class="flex items-center gap-4">
        <div class="shrink-0 overflow-hidden rounded-[10px] shadow-md">
          {#if track}
            <Artwork src={track.artworkUrl} label={track.title} size={86} radius={10} />
          {/if}
        </div>
        <div class="min-w-0 flex-1">
          <div class="line-clamp-2 text-[18px] leading-tight font-bold">
            {track?.title ?? 'Unknown track'}
          </div>
          <div class="mt-1 line-clamp-2 text-[13px]" style:color={selectedStyle.subtext}>
            {track?.artist ?? ''}
          </div>
        </div>
      </div>

      <div class="mt-5 min-h-0 flex-1 space-y-2 overflow-hidden">
        {#each shownLines as l, idx (idx)}
          <p class="line-clamp-2 text-[15px] leading-snug font-semibold">{l}</p>
        {/each}
      </div>
    </div>
  </div>

  <!-- Style Swatches (`Artwork` + 6 Presets) -->
  <div class="mt-3.5 flex flex-wrap items-center gap-2">
    {#each styles as s, idx (s.label)}
      {@const isSelected = idx === selectedIdx}
      <button
        type="button"
        onclick={() => (selectedIdx = idx)}
        use:waveTooltip={s.label}
        aria-label={s.label}
        class="h-7 w-7 cursor-pointer rounded-full transition"
        style:background={`linear-gradient(135deg, ${s.top}, ${s.bottom})`}
        style:border={isSelected
          ? '2.5px solid var(--lw-accent)'
          : '1px solid rgba(128,128,128,0.28)'}
      ></button>
    {/each}
  </div>

  <div class="mt-5 flex items-center justify-between gap-2">
    <Buttons variant="ghost" size="sm" onclick={copyQuote}>Copy text</Buttons>
    <div class="flex items-center gap-2">
      <Buttons variant="secondary" size="sm" onclick={onClose}>Cancel</Buttons>
      <Buttons variant="primary" size="sm" disabled={saving} onclick={() => void saveCardPng()}>
        {saving ? 'Saving…' : 'Save…'}
      </Buttons>
    </div>
  </div>
</dialog>
