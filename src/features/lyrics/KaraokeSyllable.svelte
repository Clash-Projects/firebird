<script lang="ts">
  import type { LyricWord } from '$src/state/types';
  import { isJoinedScript } from './lyricAdapter';

  interface Props {
    syllable: LyricWord;
    posMs: number;
    isRtl?: boolean;
    animationStyle?: string;
    animateMotion?: boolean;
    activeShadow?: string;
  }

  let {
    syllable,
    posMs,
    isRtl = false,
    animationStyle = 'apple_fluid',
    animateMotion = true,
    activeShadow = 'none',
  }: Props = $props();

  let text = $derived(syllable.text.replace(/\s+$/, ''));
  let startMs = $derived(syllable.startMs);
  let endMs = $derived(syllable.startMs + Math.max(1, syllable.endMs - syllable.startMs));

  let sylState = $derived<'idle' | 'singing' | 'sung'>(
    posMs >= endMs ? 'sung' : posMs > startMs ? 'singing' : 'idle'
  );

  let progress = $derived(
    sylState === 'sung'
      ? 1
      : sylState === 'idle'
        ? 0
        : Math.max(0, Math.min(1, (posMs - startMs) / Math.max(1, endMs - startMs)))
  );

  let joined = $derived(isJoinedScript(text));

  // Per-syllable live motion matching `_KaraokeSyllablePainter` in `karaoke_lyrics_view.dart`
  let motion = $derived.by(() => {
    if (!animateMotion || sylState !== 'singing' || progress <= 0 || progress >= 1) {
      return { transform: 'none', trailA: '', trailB: '' };
    }
    const p = progress;
    const bounce = Math.sin(Math.PI * p);
    let liftY = 0;
    let shiftX = 0;
    let scaleX = 1;
    let scaleY = 1;
    let angleRad = 0;
    let skewXRad = 0;

    switch (animationStyle) {
      case 'karaoke_bounce':
        liftY = -2.8 * bounce;
        break;
      case 'apple_fluid':
        liftY = -1.2 * bounce;
        break;
      case 'kinetic_pop':
        liftY = -5.2 * bounce;
        scaleX = scaleY = 1 + 0.13 * bounce;
        break;
      case 'neon_cyber':
        liftY = -3.4 * bounce;
        skewXRad = -0.11 * bounce;
        scaleX = scaleY = 1 + 0.05 * bounce;
        break;
      case 'italic_wave':
        liftY = -3.8 * bounce;
        angleRad = -0.045 * Math.sin(2 * Math.PI * p);
        break;
      case 'glitch_snap':
        if (p < 0.65) {
          const step = Math.floor(p * 10) % 2 === 0 ? 1 : -1;
          const decay = 1 - p / 0.65;
          shiftX = step * 1.8 * decay;
          liftY = -2.8 * decay;
        }
        break;
      case 'elastic_tilt':
        liftY = -4.2 * bounce;
        scaleX = scaleY = 1 + 0.1 * bounce;
        angleRad = 0.032 * Math.sin(2 * Math.PI * p);
        break;
      case 'hyper_warp': {
        const surge = bounce * Math.exp(-1.1 * p);
        liftY = -5.5 * bounce;
        shiftX = 2.5 * Math.sin(2 * Math.PI * p) * (1 - p);
        scaleX = 1 + 0.11 * surge;
        scaleY = 1 + 0.09 * surge;
        skewXRad = -0.17 * Math.sin(2 * Math.PI * p);
        angleRad = 0.045 * Math.sin(3 * Math.PI * p);
        break;
      }
      case 'chaos_vortex': {
        const decay = 1 - p * 0.25;
        liftY = -6.5 * bounce - 2.0 * Math.sin(3 * Math.PI * p) * (1 - p);
        shiftX = 3.0 * Math.cos(3 * Math.PI * p) * (1 - p);
        scaleX = 1 + 0.12 * bounce;
        scaleY = 1 + 0.14 * Math.abs(Math.sin(2 * Math.PI * p));
        angleRad = 0.085 * Math.sin(2.5 * Math.PI * p) * decay;
        skewXRad = 0.14 * Math.cos(2 * Math.PI * p);
        break;
      }
    }

    const buildTransform = (dx = 0, dy = 0, extraAngle = 0, extraScale = 1) => {
      const parts: string[] = [];
      const tx = shiftX + dx;
      const ty = liftY + dy;
      if (Math.abs(tx) > 0.01 || Math.abs(ty) > 0.01) {
        parts.push(`translate3d(${tx.toFixed(2)}px, ${ty.toFixed(2)}px, 0)`);
      }
      const rot = angleRad + extraAngle;
      if (Math.abs(rot) > 0.001) {
        parts.push(`rotate(${rot.toFixed(4)}rad)`);
      }
      if (Math.abs(skewXRad) > 0.001) {
        parts.push(`skewX(${skewXRad.toFixed(4)}rad)`);
      }
      const sx = scaleX * extraScale;
      const sy = scaleY * extraScale;
      if (Math.abs(sx - 1) > 0.001 || Math.abs(sy - 1) > 0.001) {
        parts.push(`scale(${sx.toFixed(3)}, ${sy.toFixed(3)})`);
      }
      return parts.length ? parts.join(' ') : 'none';
    };

    let trailA = '';
    let trailB = '';
    if (animationStyle === 'hyper_warp') {
      const trail = 1 - p;
      trailB = buildTransform(-4.5 * trail, 1.8 * trail, 0, 1.02);
      trailA = buildTransform(3.5 * trail, -1.2 * trail, 0, 1.0);
    } else if (animationStyle === 'chaos_vortex') {
      const orbitR = 5.0 * Math.sin(Math.PI * p);
      const spin = p * 4.0 * Math.PI;
      trailA = buildTransform(
        Math.cos(spin) * orbitR,
        Math.sin(spin) * orbitR,
        0.04 * Math.sin(spin),
        0.99
      );
      const spinB = spin + Math.PI;
      trailB = buildTransform(
        Math.cos(spinB) * orbitR,
        Math.sin(spinB) * orbitR,
        0.04 * Math.sin(spinB),
        0.99
      );
    }

    return { transform: buildTransform(), trailA, trailB };
  });

  // 3-band feathered wipe mask matching `_wipeFeatherBands = 3` in `karaoke_lyrics_view.dart`
  let wipeMask = $derived.by(() => {
    if (sylState !== 'singing') return 'none';
    const frontPct = Math.max(0, Math.min(100, progress * 100));
    const featherPct = Math.min(10, frontPct * 0.8, (100 - frontPct) * 0.8);
    const innerPct = Math.max(0, frontPct - featherPct);
    const mid1 = innerPct + featherPct * 0.33;
    const mid2 = innerPct + featherPct * 0.66;
    const dir = isRtl ? 'to left' : 'to right';
    return `linear-gradient(${dir}, #000 0%, #000 ${innerPct.toFixed(1)}%, rgba(0,0,0,0.68) ${mid1.toFixed(1)}%, rgba(0,0,0,0.32) ${mid2.toFixed(1)}%, transparent ${frontPct.toFixed(1)}%, transparent 100%)`;
  });

  // Compact Canvas VFX for `hyper_warp` (shockwaves + 8 speed streaks) and `chaos_vortex` (prism ring + 8 confetti shards)
  let vfxCanvas = $state<HTMLCanvasElement | null>(null);
  let usesVfx = $derived(
    animateMotion && sylState === 'singing' && (animationStyle === 'hyper_warp' || animationStyle === 'chaos_vortex')
  );

  $effect(() => {
    if (!usesVfx || !vfxCanvas) return;
    const p = progress;
    const ctx = vfxCanvas.getContext('2d');
    if (!ctx) return;
    const w = vfxCanvas.width;
    const h = vfxCanvas.height;
    ctx.clearRect(0, 0, w, h);

    const cx = w * 0.5;
    const cy = h * 0.5;
    const baseRadius = Math.min(w, h) * 0.28;

    if (animationStyle === 'hyper_warp') {
      const fadeOut = Math.max(0, Math.min(1, 1 - p));
      const ringRadius = baseRadius * (0.35 + 0.85 * p);

      ctx.beginPath();
      ctx.arc(cx, cy, ringRadius, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(0, 245, 255, ${(0.55 * fadeOut).toFixed(3)})`;
      ctx.lineWidth = Math.max(0.5, Math.min(2.5, 2.2 * fadeOut));
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(cx, cy, ringRadius * 0.72, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(184, 41, 255, ${(0.42 * fadeOut).toFixed(3)})`;
      ctx.lineWidth = Math.max(0.5, Math.min(2.0, 1.5 * fadeOut));
      ctx.stroke();

      ctx.strokeStyle = `rgba(0, 245, 255, ${(0.65 * fadeOut).toFixed(3)})`;
      ctx.lineWidth = Math.max(0.5, 1.5 * fadeOut);
      ctx.lineCap = 'round';
      for (let k = 0; k < 8; k++) {
        const theta = (k / 8) * Math.PI * 2 + p * 1.2;
        const r0 = baseRadius * (0.5 + 0.45 * p);
        const r1 = r0 + 10 * (0.4 + p);
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(theta) * r0, cy + Math.sin(theta) * r0);
        ctx.lineTo(cx + Math.cos(theta) * r1, cy + Math.sin(theta) * r1);
        ctx.stroke();
      }
    } else if (animationStyle === 'chaos_vortex') {
      const fadeOut = Math.max(0, Math.min(1, 1 - p * 0.85));
      ctx.beginPath();
      ctx.arc(cx, cy, baseRadius * (0.4 + 0.8 * p), 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(255, 30, 110, ${(0.55 * fadeOut).toFixed(3)})`;
      ctx.lineWidth = Math.max(0.5, 1.8 * fadeOut);
      ctx.stroke();

      for (let k = 0; k < 8; k++) {
        const theta = (k / 8) * Math.PI * 2 + p * 3.5;
        const dist = baseRadius * (0.45 + 0.85 * p);
        const sx = cx + Math.cos(theta) * dist;
        const sy = cy + Math.sin(theta) * dist;
        ctx.save();
        ctx.translate(sx, sy);
        ctx.rotate(theta + p * 6.0);
        ctx.fillStyle =
          k % 2 === 0
            ? `rgba(255, 30, 110, ${(0.78 * fadeOut).toFixed(3)})`
            : `rgba(255, 208, 0, ${(0.78 * fadeOut).toFixed(3)})`;
        ctx.fillRect(-2.5, -1.5, 5, 3);
        ctx.restore();
      }
    }
  });
</script>

{#if text}
  <span
    data-singing={sylState === 'singing' ? '' : undefined}
    class="relative inline-block select-none"
  ><span
      class="inline-block text-[var(--lw-lyric-ink)]"
      style:opacity={sylState === 'sung' ? 1 : 0.38}
      style:text-shadow={sylState === 'sung' ? activeShadow : 'none'}
    >{text}</span>{#if sylState === 'singing'}<span aria-hidden="true" class="pointer-events-none absolute inset-0">
      {#if usesVfx}
        <canvas
          bind:this={vfxCanvas}
          width={120}
          height={84}
          aria-hidden="true"
          class="pointer-events-none absolute top-1/2 left-1/2 z-0 h-[84px] w-[120px] -translate-x-1/2 -translate-y-1/2"
        ></canvas>
      {/if}

      {#if animationStyle === 'hyper_warp' && motion.trailA}
        <span
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 z-1 inline-block whitespace-pre"
          style:color="rgba(184, 41, 255, 0.72)"
          style:transform={motion.trailB}
          style:transform-origin="center center"
        >{text}</span>
        <span
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 z-1 inline-block whitespace-pre"
          style:color="rgba(0, 245, 255, 0.72)"
          style:transform={motion.trailA}
          style:transform-origin="center center"
        >{text}</span>
      {:else if animationStyle === 'chaos_vortex' && motion.trailA}
        <span
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 z-1 inline-block whitespace-pre"
          style:color="rgba(255, 30, 110, 0.75)"
          style:transform={motion.trailA}
          style:transform-origin="center center"
        >{text}</span>
        <span
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 z-1 inline-block whitespace-pre"
          style:color="rgba(255, 208, 0, 0.75)"
          style:transform={motion.trailB}
          style:transform-origin="center center"
        >{text}</span>
      {/if}

      <!-- Active feathered wipe layer -->
      <span
        aria-hidden="true"
        class="pointer-events-none absolute inset-0 z-2 inline-block whitespace-pre text-[var(--lw-lyric-ink)]"
        style:text-shadow={joined ? 'none' : activeShadow}
        style:transform={motion.transform}
        style:transform-origin="center center"
        style:-webkit-mask-image={wipeMask}
        style:mask-image={wipeMask}
      >{text}</span>
    </span>{/if}</span>
{/if}
