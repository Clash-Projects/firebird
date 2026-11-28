<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { dismissFlyout } from '$src/app/accessibility';
  import {
    featuresStore,
    LYRICS_ANIMATION_OPTIONS,
    LYRICS_AUTO_HIDE_OPTIONS,
    LYRICS_PROVIDERS,
    LYRICS_TEXT_SCALES,
    LYRICS_TRANSLATION_OPTIONS,
    lyricsSourceToProviderId,
  } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { LyricLine } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import LyricsShareCard from '$src/ui/lyrics/LyricsShareCard.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import KaraokeSyllable from './KaraokeSyllable.svelte';
  import {
    activeLineIndex,
    computeVocalRoles,
    groupSyllablesIntoWords,
    interpolateLineWords,
    isJoinedScript,
    isRtlText,
    lyricFollowAlignment,
    lyricsAreUntimed,
  } from './lyricAdapter';

  interface Props {
    compact?: boolean;
    fontSize?: number;
    showHeaderControls?: boolean;
    toolbarRightInset?: number;
    onClose?: () => void;
  }

  let {
    compact = false,
    fontSize,
    showHeaderControls = true,
    toolbarRightInset = 0,
    onClose,
  }: Props = $props();

  let track = $derived(playerStore.currentTrack);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let errorMsg = $derived(track ? featuresStore.lyricsErrorByTrack[track.id] : undefined);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  let baseFontSize = $derived((fontSize ?? (compact ? 26 : 36)) * featuresStore.lyricsFontScale);
  let animStyle = $derived(featuresStore.lyricsAnimation);

  // Follow-scroll & Multi-line Share Card selection state
  let following = $state(true);
  let selectMode = $state(false);
  let selectedIndices = $state<number[]>([]);
  let shareLines = $state<string[] | null>(null);

  // Toolbar Menus & Auto-hide state
  let sourceMenuOpen = $state(false);
  let moreMenuOpen = $state(false);
  type MoreSubmenu = 'textSize' | 'translation' | 'autoHide' | 'animation' | null;
  let moreSubmenu = $state<MoreSubmenu>(null);
  let hoverSubTimer: ReturnType<typeof setTimeout> | null = null;
  let closeSubTimer: ReturnType<typeof setTimeout> | null = null;

  let controlsShown = $state(true);
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  let lastPokeAt = 0;

  function pokeControls() {
    const secs = featuresStore.lyricsAutoHideSeconds;
    lastPokeAt = Date.now();
    if (!controlsShown) controlsShown = true;
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (selectMode || sourceMenuOpen || moreMenuOpen || secs <= 0) return;
    hideTimer = setTimeout(onHideTimeout, secs * 1000);
  }

  function onHideTimeout() {
    if (selectMode || sourceMenuOpen || moreMenuOpen || featuresStore.lyricsAutoHideSeconds <= 0) {
      return;
    }
    const remaining = featuresStore.lyricsAutoHideSeconds * 1000 - (Date.now() - lastPokeAt);
    if (remaining > 40) {
      hideTimer = setTimeout(onHideTimeout, remaining);
      return;
    }
    controlsShown = false;
  }

  $effect(() => {
    void featuresStore.lyricsAutoHideSeconds;
    pokeControls();
  });

  // Reset selection & follow when track changes
  let lastTrackId = $state<string | null>(null);
  $effect(() => {
    const tid = track?.id ?? null;
    if (tid !== lastTrackId) {
      lastTrackId = tid;
      selectMode = false;
      selectedIndices = [];
      following = true;
    }
  });

  // Precision 60fps interpolated playback clock (`audioMs - offsetMs` matching OG Flutter!)
  let playbackAnchor = $state.raw({
    trackId: null as string | null,
    seekRev: 0,
    audioMs: 0,
    sampledAt: 0,
    playing: false,
    speed: 1,
  });
  let liveAudioMs = $state(0);

  $effect(() => {
    const tid = track?.id ?? null;
    const seekRev = playerStore.seekRevision;
    const speed = playerStore.speed > 0 ? playerStore.speed : 1;
    const playing = playerStore.isPlaying && !playerStore.isBuffering;
    const audioMs = playerStore.positionSec * 1000;
    const now = performance.now();
    const prev = untrack(() => playbackAnchor);
    const predicted = prev.audioMs + (prev.playing ? (now - prev.sampledAt) * prev.speed : 0);
    const jumped =
      tid !== prev.trackId ||
      seekRev !== prev.seekRev ||
      playing !== prev.playing ||
      speed !== prev.speed ||
      Math.abs(audioMs - predicted) > 85;

    if (jumped || !playing) {
      playbackAnchor = { trackId: tid, seekRev, audioMs, sampledAt: now, playing, speed };
      liveAudioMs = audioMs;
    }
  });

  let untimed = $derived(!doc?.synced || (doc ? lyricsAreUntimed(doc.lines) : true));
  let isWordKaraoke = $derived(Boolean(doc?.synced && !untimed && featuresStore.wordByWord));
  let usesReactor = $derived(animStyle === 'hyper_warp' || animStyle === 'chaos_vortex');

  $effect(() => {
    const anchor = playbackAnchor;
    if (!anchor.playing || !doc?.synced || untimed) return;

    let rafId = 0;
    let wakeTimer: ReturnType<typeof setTimeout> | null = null;
    let lastFrame = 0;

    const tick = (now: number) => {
      if (now - lastFrame >= 15) {
        lastFrame = now;
        const currentAudio = anchor.audioMs + Math.max(0, now - anchor.sampledAt) * anchor.speed;
        liveAudioMs = currentAudio;

        // Sleep during pre-cue intros or long inter-line breaks when no reactor/dots need rAF
        if (!isWordKaraoke && !usesReactor && doc && doc.lines.length > 0) {
          const effMs = currentAudio - offsetMs;
          const idx = activeLineIndex(doc.lines, effMs, true);
          const nextLine = doc.lines[idx + 1];
          if (nextLine) {
            const delayMs = Math.max(16, Math.round((nextLine.startMs - effMs - 40) / anchor.speed));
            if (delayMs > 220 && delayMs < 120_000) {
              wakeTimer = setTimeout(() => {
                wakeTimer = null;
                rafId = requestAnimationFrame(tick);
              }, delayMs);
              return;
            }
          }
        }
      }
      rafId = requestAnimationFrame(tick);
    };

    rafId = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(rafId);
      if (wakeTimer) clearTimeout(wakeTimer);
    };
  });

  // Effective lyrics timestamp (`audioMs - offsetMs` matching OG Flutter `karaoke_lyrics_view.dart`)
  let posMs = $derived(liveAudioMs - offsetMs);
  let activeIdx = $derived(doc ? activeLineIndex(doc.lines, posMs, doc.synced) : -1);

  // Pulsing phase (0..1..0 on 1400ms cycle) for glowing `♪ • • •` instrumental break dots
  let breakPulse = $derived(0.5 - 0.5 * Math.cos(((Math.max(0, liveAudioMs) % 1400) / 1400) * Math.PI * 2));

  // Spatial Duet roles
  let vocalRoles = $derived(doc && track ? computeVocalRoles(doc.lines, track.artist) : []);

  // Style-specific typography shared identically across idle & active so font metrics never snap/rewrap
  let lineFontWeight = $derived(
    animStyle === 'hyper_warp' || animStyle === 'chaos_vortex'
      ? 900
      : animStyle === 'kinetic_pop' || animStyle === 'glitch_snap' || animStyle === 'elastic_tilt'
        ? 800
        : 700
  );
  let lineFontStyle = $derived(
    animStyle === 'italic_wave' || animStyle === 'chaos_vortex' ? 'italic' : 'normal'
  );
  let baseStyleTracking = $derived.by(() => {
    switch (animStyle) {
      case 'kinetic_pop':
        return -0.5;
      case 'neon_cyber':
        return 0.55;
      case 'italic_wave':
        return 0.35;
      case 'glitch_snap':
        return 0.25;
      case 'elastic_tilt':
        return -0.4;
      case 'hyper_warp':
        return -0.8;
      case 'chaos_vortex':
        return 0.6;
      default:
        return -0.3;
    }
  });

  let activeShadow = $derived.by(() => {
    switch (animStyle) {
      case 'apple_fluid':
        return '0 0 10px color-mix(in srgb, var(--lw-lyric-ink) 38%, transparent)';
      case 'karaoke_bounce':
        return '0 0 6px color-mix(in srgb, var(--lw-lyric-ink) 30%, transparent)';
      case 'kinetic_pop':
        return '0 4px 10px rgba(0, 0, 0, 0.52), 0 0 6px color-mix(in srgb, var(--lw-lyric-ink) 28%, transparent)';
      case 'neon_cyber':
        return '0 0 10px color-mix(in srgb, var(--lw-accent) 90%, transparent), 0 0 24px color-mix(in srgb, var(--lw-accent) 55%, transparent)';
      case 'italic_wave':
        return '0 1px 14px color-mix(in srgb, var(--lw-lyric-ink) 45%, transparent)';
      case 'glitch_snap':
        return '-2.2px 0 1.5px rgba(0, 240, 255, 0.8), 2.2px 0 1.5px rgba(255, 42, 133, 0.8)';
      case 'elastic_tilt':
        return '0 0 14px color-mix(in srgb, var(--lw-accent) 50%, transparent), 2px 5px 12px rgba(0, 0, 0, 0.5)';
      case 'hyper_warp':
        return '-1.5px 0 6px rgba(0, 245, 255, 0.72), 1.5px 1px 14px rgba(123, 47, 255, 0.65)';
      case 'chaos_vortex':
        return '-1.8px -0.8px 5px rgba(255, 30, 110, 0.70), 1.8px 1px 10px rgba(255, 208, 0, 0.62)';
      default:
        return 'none';
    }
  });

  function activeLineTransform(isRtl: boolean): string {
    const dir = isRtl ? -1 : 1;
    if (animStyle === 'hyper_warp') {
      const t = ((Math.max(0, posMs) % 3200) / 3200) * 2 * Math.PI;
      const pulse = 1.02 * (1.008 + 0.01 * Math.sin(t * 2));
      const ty = -1.5 - 1.5 * Math.cos(t * 2);
      const rx = -0.04 + 0.03 * Math.cos(t);
      const ry = 0.04 * Math.sin(t) * dir;
      const rz = 0.01 * Math.sin(t * 2) * dir;
      return `perspective(714px) translate3d(0, ${ty.toFixed(2)}px, 0) rotateX(${rx.toFixed(4)}rad) rotateY(${ry.toFixed(4)}rad) rotateZ(${rz.toFixed(4)}rad) scale(${pulse.toFixed(3)})`;
    }
    if (animStyle === 'chaos_vortex') {
      const t = ((Math.max(0, posMs) % 2400) / 2400) * 2 * Math.PI;
      const sx = 1.02 * (1.006 + 0.012 * Math.sin(t * 2));
      const sy = 1.02 * (1.006 + 0.012 * Math.cos(t * 2));
      const ty = -2.0 - 2.0 * Math.abs(Math.sin(t * 3));
      const skew = 0.03 * Math.cos(t * 2) * dir;
      const ry = (0.05 + 0.06 * Math.sin(t)) * dir;
      const rx = 0.04 * Math.cos(t);
      const rz = 0.016 * Math.sin(t * 2) * dir;
      return `perspective(625px) translate3d(0, ${ty.toFixed(2)}px, 0) rotateY(${ry.toFixed(4)}rad) rotateX(${rx.toFixed(4)}rad) rotateZ(${rz.toFixed(4)}rad) skewX(${skew.toFixed(4)}rad) scale(${sx.toFixed(3)}, ${sy.toFixed(3)})`;
    }
    switch (animStyle) {
      case 'kinetic_pop':
        return 'scale(1.03)';
      case 'neon_cyber':
        return `scale(1.025) skewX(${(-3.15 * dir).toFixed(2)}deg)`;
      case 'italic_wave':
        return 'translate3d(0, -2px, 0) scale(1.025)';
      case 'glitch_snap':
        return 'scale(1.025)';
      case 'elastic_tilt':
        return `perspective(833px) rotateY(${(-3.72 * dir).toFixed(2)}deg) scale(1.03)`;
      default:
        return 'scale(1.02)';
    }
  }

  let lineTransitionDuration = $derived.by(() => {
    switch (animStyle) {
      case 'glitch_snap':
        return '170ms';
      case 'hyper_warp':
      case 'chaos_vortex':
        return '60ms';
      case 'kinetic_pop':
      case 'elastic_tilt':
        return '340ms';
      default:
        return '240ms';
    }
  });

  let lineTransitionCurve = $derived.by(() => {
    switch (animStyle) {
      case 'kinetic_pop':
      case 'elastic_tilt':
        return 'cubic-bezier(0.34, 1.56, 0.64, 1)';
      case 'glitch_snap':
        return 'cubic-bezier(0.16, 1, 0.3, 1)';
      default:
        return 'cubic-bezier(0.22, 1, 0.36, 1)';
    }
  });

  // Smooth Apple Music Spring Scroll (`400ms`, `cubic-bezier(0.41, 0.0, 0.12, 0.99)`, 18% upper-third anchor)
  let scrollEl = $state<HTMLDivElement | null>(null);
  let scrollAnimId = 0;
  let programmaticScrollUntil = 0;
  let lastScrolledIdx = -2;
  let lastScrolledTrack = '';

  function scrollToIndex(idx: number, animate = true) {
    if (!scrollEl || !doc || doc.lines.length === 0) return;
    const clampedIdx = Math.max(0, Math.min(doc.lines.length - 1, idx));
    const targetNode = scrollEl.querySelector<HTMLElement>(`[data-lyric-idx="${clampedIdx}"]`);
    if (!targetNode) return;

    const viewH = scrollEl.clientHeight;
    const maxScroll = Math.max(0, scrollEl.scrollHeight - viewH);
    if (maxScroll <= 0) return;

    const anchor = lyricFollowAlignment(clampedIdx, compact, 0.18);
    const nodeTop = targetNode.offsetTop;
    const desiredTop = Math.max(0, Math.min(maxScroll, nodeTop - viewH * anchor));

    cancelAnimationFrame(scrollAnimId);
    const reduced =
      settingsStore.reduceMotion ||
      (typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches);

    if (!animate || reduced || Math.abs(desiredTop - scrollEl.scrollTop) < 2) {
      programmaticScrollUntil = performance.now() + 120;
      scrollEl.scrollTop = desiredTop;
      return;
    }

    const startTop = scrollEl.scrollTop;
    const delta = desiredTop - startTop;
    const duration = 400;
    const startTime = performance.now();
    programmaticScrollUntil = startTime + duration + 120;

    // Cubic(0.41, 0.0, 0.12, 0.99) approximation
    const springEase = (t: number): number => {
      if (t <= 0) return 0;
      if (t >= 1) return 1;
      // Solve x(u) = t for P1=(0.41,0), P2=(0.12,0.99) via 5 Newton steps
      let u = t;
      for (let k = 0; k < 5; k++) {
        const inv = 1 - u;
        const x = 3 * inv * inv * u * 0.41 + 3 * inv * u * u * 0.12 + u * u * u;
        const dx = 3 * inv * inv * 0.41 + 6 * inv * u * (0.12 - 0.41) + 3 * u * u * (1 - 0.12);
        if (Math.abs(dx) < 1e-5) break;
        u = Math.max(0, Math.min(1, u - (x - t) / dx));
      }
      const inv = 1 - u;
      return 3 * inv * u * u * 0.99 + u * u * u;
    };

    const step = (now: number) => {
      if (!scrollEl) return;
      const elapsed = Math.min(1, (now - startTime) / duration);
      programmaticScrollUntil = now + 80;
      scrollEl.scrollTop = startTop + delta * springEase(elapsed);
      if (elapsed < 1) {
        scrollAnimId = requestAnimationFrame(step);
      }
    };
    scrollAnimId = requestAnimationFrame(step);
  }

  $effect(() => {
    const tid = doc?.trackId ?? '';
    const idx = activeIdx;
    const isFollow = following;
    if (!doc || !isFollow || untimed) return;

    const trackChanged = tid !== lastScrolledTrack;
    if (trackChanged) {
      lastScrolledTrack = tid;
      lastScrolledIdx = idx;
      queueMicrotask(() => scrollToIndex(Math.max(0, idx), false));
      return;
    }

    if (idx !== lastScrolledIdx) {
      const wasPreCue = lastScrolledIdx < 0;
      lastScrolledIdx = idx;
      scrollToIndex(Math.max(0, idx), !wasPreCue && idx > 0);
    }
  });

  function handleUserWheel() {
    if (following && doc?.synced && !untimed) {
      cancelAnimationFrame(scrollAnimId);
      following = false;
    }
  }

  function handleTouchMove() {
    if (performance.now() > programmaticScrollUntil && following && doc?.synced && !untimed) {
      cancelAnimationFrame(scrollAnimId);
      following = false;
    }
  }

  function resumeFollowing() {
    following = true;
    scrollToIndex(Math.max(0, activeIdx), true);
  }

  function seekToLine(line: LyricLine, idx: number) {
    if (selectMode) {
      if (selectedIndices.includes(idx)) {
        selectedIndices = selectedIndices.filter((i) => i !== idx);
      } else {
        selectedIndices = [...selectedIndices, idx];
      }
      return;
    }
    if (!doc?.synced || untimed) return;
    // Matches OG Flutter: `seekTargetMs = lineMs + _offsetMs`
    const seekSec = Math.max(0, (line.startMs + offsetMs) / 1000);
    playerStore.seek(seekSec);
  }

  function toggleSelectMode() {
    selectMode = !selectMode;
    if (selectMode) {
      following = false;
    } else {
      selectedIndices = [];
    }
    moreMenuOpen = false;
    pokeControls();
  }

  function shareSongExcerpt() {
    if (!doc || doc.lines.length === 0) return;
    const idx = activeIdx;
    let slice: LyricLine[];
    if (idx < 0 || idx >= doc.lines.length) {
      slice = doc.lines.slice(0, 8);
    } else {
      const start = Math.max(0, idx - 2);
      const end = Math.min(doc.lines.length, idx + 6);
      slice = doc.lines.slice(start, end);
    }
    shareLines = slice.map((l) => l.text).filter(Boolean);
    moreMenuOpen = false;
  }

  function shareSelectedLines() {
    if (!doc || selectedIndices.length === 0) return;
    const sorted = [...selectedIndices]
      .filter((i) => i >= 0 && i < doc!.lines.length)
      .sort((a, b) => a - b)
      .slice(0, 12);
    shareLines = sorted.map((i) => doc!.lines[i].text).filter(Boolean);
    moreMenuOpen = false;
  }

  function formatOffset(ms: number): string {
    if (ms === 0) return '0.0s';
    const sign = ms > 0 ? '+' : '-';
    return `${sign}${(Math.abs(ms) / 1000).toFixed(1)}s`;
  }

  function lineWordsForKaraoke(line: LyricLine) {
    if (line.words && line.words.length > 0) {
      return groupSyllablesIntoWords(line.words, line.text);
    }
    const interpolated = interpolateLineWords(
      line.text,
      line.startMs,
      Math.max(80, line.endMs - line.startMs)
    );
    return groupSyllablesIntoWords(interpolated, line.text);
  }

  function prevEndForBreak(lines: LyricLine[], idx: number): number {
    if (idx <= 0) return 0;
    const prev = lines[idx - 1];
    if (prev.words && prev.words.length > 0) {
      const lastW = prev.words[prev.words.length - 1];
      return lastW.endMs;
    }
    return Math.max(prev.endMs, prev.startMs + 1000);
  }

  function distanceOpacity(idx: number, active: number): number {
    if (!following || selectMode || untimed) return 1;
    if (idx === active) return 1;
    const ref = active >= 0 ? active : 0;
    const dist = Math.abs(idx - ref);
    const isPast = active >= 0 && idx < active;
    if (dist === 1) return isPast ? 0.45 : 0.55;
    if (dist === 2) return isPast ? 0.35 : 0.42;
    return 0.35;
  }

  let toolbarStatusTitle = $derived.by(() => {
    if (!doc) return 'Lyrics';
    if (doc.isInstrumental) return 'Instrumental';
    if (featuresStore.wordByWord) {
      if (doc.wordSynced) return 'Karaoke · Syllable Synced';
      if (doc.synced) return 'Synced Lyrics';
      return doc.provider || 'Lyrics';
    }
    if (doc.synced) return 'Apple Music · Line Synced';
    return doc.provider || 'Lyrics';
  });

  let effectiveSelectedProvider = $derived.by(() => {
    if (doc?.provider) {
      const resolved = lyricsSourceToProviderId(doc.provider);
      if (resolved) return resolved;
    }
    return featuresStore.getEffectiveProviderId(track?.id);
  });

  function hoverMoreSubmenu(sub: MoreSubmenu) {
    if (closeSubTimer) {
      clearTimeout(closeSubTimer);
      closeSubTimer = null;
    }
    if (hoverSubTimer) clearTimeout(hoverSubTimer);
    if (moreSubmenu === sub) return;
    hoverSubTimer = setTimeout(() => {
      moreSubmenu = sub;
    }, 160);
  }

  function keepMoreSubmenu() {
    if (hoverSubTimer) {
      clearTimeout(hoverSubTimer);
      hoverSubTimer = null;
    }
    if (closeSubTimer) {
      clearTimeout(closeSubTimer);
      closeSubTimer = null;
    }
  }

  function scheduleCloseMoreSubmenu() {
    if (hoverSubTimer) {
      clearTimeout(hoverSubTimer);
      hoverSubTimer = null;
    }
    if (closeSubTimer) clearTimeout(closeSubTimer);
    closeSubTimer = setTimeout(() => {
      moreSubmenu = null;
    }, 280);
  }

  onDestroy(() => {
    cancelAnimationFrame(scrollAnimId);
    if (hideTimer) clearTimeout(hideTimer);
    if (hoverSubTimer) clearTimeout(hoverSubTimer);
    if (closeSubTimer) clearTimeout(closeSubTimer);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  data-lw="lyrics-view"
  data-animation={animStyle}
  onpointerenter={pokeControls}
  onpointermove={pokeControls}
  class="lw-karaoke-root relative flex h-full min-h-0 w-full flex-col"
>
  <!-- 1. OG Flutter `_KaraokeToolbar` (auto-hides on idle, stays visible on empty/instrumental so source switcher works) -->
  {#if showHeaderControls && track}
    {@const hasLyrics = Boolean(doc && !doc.isInstrumental && doc.lines.length > 0)}
    <div
      class="shrink-0 transition-opacity duration-180"
      class:pointer-events-none={!controlsShown}
      style:opacity={controlsShown ? 1 : 0}
      style:padding={`${compact ? 4 : 4}px ${(compact ? 8 : 4) + toolbarRightInset}px 8px ${compact ? 8 : 4}px`}
    >
      <div
        class="flex items-center gap-1.5 rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-panel-translucent)] px-3 py-1.5"
      >
        <span class="min-w-0 flex-1 truncate text-[12px] font-semibold text-[var(--lw-text-primary)]">
          {toolbarStatusTitle}
        </span>

        <!-- `_SourceMenu` -->
        <div
          class="relative"
          use:dismissFlyout={{
            open: sourceMenuOpen,
            close: () => {
              sourceMenuOpen = false;
              pokeControls();
            },
          }}
        >
          <button
            type="button"
            onclick={() => {
              moreMenuOpen = false;
              sourceMenuOpen = !sourceMenuOpen;
              pokeControls();
            }}
            use:waveTooltip={doc?.provider ? `Source: ${doc.provider} · Change` : 'Change lyrics source'}
            class="flex h-[26px] w-[26px] cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-primary)]/90 transition hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="lyrics" size={13} />
          </button>

          <div
            data-open={sourceMenuOpen}
            inert={!sourceMenuOpen}
            class="lw-popover lw-flyout-surface absolute top-8 right-0 z-50 max-h-[360px] w-[236px] overflow-y-auto"
          >
            {#each LYRICS_PROVIDERS as p (p.id)}
              {@const checked = effectiveSelectedProvider === p.id}
              <button
                type="button"
                onclick={() => {
                  featuresStore.selectTrackLyricsProvider(track, p.id);
                  sourceMenuOpen = false;
                  pokeControls();
                }}
                class="lw-menu-row"
              >
                <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                  {#if checked}<WaveIcon name="check" size={12} />{/if}
                </span>
                <span class="min-w-0 flex-1 truncate">{p.label}</span>
              </button>
            {/each}
            <div class="lw-menu-sep"></div>
            <button
              type="button"
              onclick={() => {
                featuresStore.selectTrackLyricsProvider(track, null);
                sourceMenuOpen = false;
                pokeControls();
              }}
              class="lw-menu-row"
            >
              <WaveIcon name="refresh" size={13} />
              <span class="min-w-0 flex-1 truncate">Try another source</span>
            </button>
          </div>
        </div>

        <!-- Timing Offset Pill `[-] 0.0s [+] [Reset]` -->
        {#if hasLyrics && doc?.synced && !untimed}
          <div
            class="flex items-center gap-0.5 rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-wash-hover)] px-1 py-0.5 lw-tabular"
          >
            <button
              type="button"
              onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
              use:waveTooltip={'Lyrics earlier (-0.5s)'}
              class="flex h-[22px] w-[22px] cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-primary)]/90 hover:bg-[var(--lw-wash-hover)]"
            ><WaveIcon name="remove" size={11} /></button>
            <span
              use:waveTooltip={'Timing offset for this track'}
              class="px-1 text-[11px] font-semibold {offsetMs !== 0 ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]'}"
            >
              {formatOffset(offsetMs)}
            </span>
            <button
              type="button"
              onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
              use:waveTooltip={'Lyrics later (+0.5s)'}
              class="flex h-[22px] w-[22px] cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-primary)]/90 hover:bg-[var(--lw-wash-hover)]"
            ><WaveIcon name="plus" size={11} /></button>
            {#if offsetMs !== 0}
              <button
                type="button"
                onclick={() => featuresStore.resetLyricsOffset(track.id)}
                use:waveTooltip={'Reset offset to 0.0s'}
                class="flex h-[22px] w-[22px] cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-primary)]/90 hover:bg-[var(--lw-wash-hover)]"
              >
                <WaveIcon name="reset" size={11} />
              </button>
            {/if}
          </div>
        {/if}

        <!-- `More (...)` Menu -->
        {#if hasLyrics}
          <div
            class="relative"
            use:dismissFlyout={{
              open: moreMenuOpen,
              close: () => {
                moreMenuOpen = false;
                moreSubmenu = null;
                pokeControls();
              },
            }}
          >
            <button
              type="button"
              onclick={() => {
                sourceMenuOpen = false;
                moreMenuOpen = !moreMenuOpen;
                moreSubmenu = null;
                pokeControls();
              }}
              use:waveTooltip={'More'}
              class="flex h-[26px] w-[26px] cursor-pointer items-center justify-center rounded-[10px] transition hover:bg-[var(--lw-wash-hover)] {selectMode ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]/90'}"
            >
              <WaveIcon name="more" size={13} />
            </button>

            <div
              data-open={moreMenuOpen}
              inert={!moreMenuOpen}
              class="lw-popover lw-has-cascade lw-flyout-surface absolute top-8 right-0 z-50 w-[232px]"
            >
              <button
                type="button"
                onpointerenter={scheduleCloseMoreSubmenu}
                onclick={() => featuresStore.setLyricsRomanized(!featuresStore.showRomanized)}
                class="lw-menu-row"
              >
                <WaveIcon name="readingMode" size={13} />
                <span class="min-w-0 flex-1 truncate">Romanisation</span>
                <span class="lw-menu-hint">{featuresStore.showRomanized ? 'On' : 'Off'}</span>
              </button>

              <!-- Text size submenu -->
              <div
                role="presentation"
                class="relative"
                onpointerenter={() => hoverMoreSubmenu('textSize')}
                onpointerleave={scheduleCloseMoreSubmenu}
              >
                <button
                  type="button"
                  data-highlighted={moreSubmenu === 'textSize'}
                  onclick={() => (moreSubmenu = moreSubmenu === 'textSize' ? null : 'textSize')}
                  class="lw-menu-row"
                >
                  <WaveIcon name="fontSize" size={13} />
                  <span class="min-w-0 flex-1 truncate">Text size</span>
                  <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
                </button>
                {#if moreSubmenu === 'textSize'}
                  <div
                    role="menu"
                    tabindex="-1"
                    onpointerenter={keepMoreSubmenu}
                    onpointerleave={scheduleCloseMoreSubmenu}
                    class="absolute top-[-4px] right-full z-50 w-[184px] pr-1.5"
                  >
                    <div class="lw-flyout-surface">
                      {#each LYRICS_TEXT_SCALES as s (s.scale)}
                        <button
                          type="button"
                          onclick={() => {
                            featuresStore.setLyricsTextScale(s.scale);
                            moreMenuOpen = false;
                            moreSubmenu = null;
                          }}
                          class="lw-menu-row"
                        >
                          <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                            {#if Math.abs(featuresStore.lyricsFontScale - s.scale) < 0.04}<WaveIcon name="check" size={12} />{/if}
                          </span>
                          <span class="min-w-0 flex-1 truncate">{s.label}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>

              <!-- Translation submenu -->
              <div
                role="presentation"
                class="relative"
                onpointerenter={() => hoverMoreSubmenu('translation')}
                onpointerleave={scheduleCloseMoreSubmenu}
              >
                <button
                  type="button"
                  data-highlighted={moreSubmenu === 'translation'}
                  onclick={() => (moreSubmenu = moreSubmenu === 'translation' ? null : 'translation')}
                  class="lw-menu-row"
                >
                  <WaveIcon name="localeLanguage" size={13} />
                  <span class="min-w-0 flex-1 truncate">Translation</span>
                  <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
                </button>
                {#if moreSubmenu === 'translation'}
                  <div
                    role="menu"
                    tabindex="-1"
                    onpointerenter={keepMoreSubmenu}
                    onpointerleave={scheduleCloseMoreSubmenu}
                    class="absolute top-[-4px] right-full z-50 w-[196px] pr-1.5"
                  >
                    <div class="lw-flyout-surface max-h-[320px] overflow-y-auto">
                      {#each LYRICS_TRANSLATION_OPTIONS as opt (opt.id)}
                        <button
                          type="button"
                          onclick={() => {
                            featuresStore.setLyricsTranslationLang(opt.id);
                            moreMenuOpen = false;
                            moreSubmenu = null;
                          }}
                          class="lw-menu-row"
                        >
                          <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                            {#if featuresStore.lyricsTranslationLang === opt.id}<WaveIcon name="check" size={12} />{/if}
                          </span>
                          <span class="min-w-0 flex-1 truncate">{opt.label}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>

              <!-- Auto-hide submenu -->
              <div
                role="presentation"
                class="relative"
                onpointerenter={() => hoverMoreSubmenu('autoHide')}
                onpointerleave={scheduleCloseMoreSubmenu}
              >
                <button
                  type="button"
                  data-highlighted={moreSubmenu === 'autoHide'}
                  onclick={() => (moreSubmenu = moreSubmenu === 'autoHide' ? null : 'autoHide')}
                  class="lw-menu-row"
                >
                  <WaveIcon name="clock" size={13} />
                  <span class="min-w-0 flex-1 truncate">Auto-hide</span>
                  <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
                </button>
                {#if moreSubmenu === 'autoHide'}
                  <div
                    role="menu"
                    tabindex="-1"
                    onpointerenter={keepMoreSubmenu}
                    onpointerleave={scheduleCloseMoreSubmenu}
                    class="absolute top-[-4px] right-full z-50 w-[184px] pr-1.5"
                  >
                    <div class="lw-flyout-surface">
                      {#each LYRICS_AUTO_HIDE_OPTIONS as opt (opt.seconds)}
                        <button
                          type="button"
                          onclick={() => {
                            featuresStore.setLyricsAutoHideSeconds(opt.seconds);
                            moreMenuOpen = false;
                            moreSubmenu = null;
                            pokeControls();
                          }}
                          class="lw-menu-row"
                        >
                          <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                            {#if featuresStore.lyricsAutoHideSeconds === opt.seconds}<WaveIcon name="check" size={12} />{/if}
                          </span>
                          <span class="min-w-0 flex-1 truncate">{opt.label}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>

              <!-- Animation style submenu -->
              <div
                role="presentation"
                class="relative"
                onpointerenter={() => hoverMoreSubmenu('animation')}
                onpointerleave={scheduleCloseMoreSubmenu}
              >
                <button
                  type="button"
                  data-highlighted={moreSubmenu === 'animation'}
                  onclick={() => (moreSubmenu = moreSubmenu === 'animation' ? null : 'animation')}
                  class="lw-menu-row"
                >
                  <WaveIcon name="brush" size={13} />
                  <span class="min-w-0 flex-1 truncate">Animation style</span>
                  <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
                </button>
                {#if moreSubmenu === 'animation'}
                  <div
                    role="menu"
                    tabindex="-1"
                    onpointerenter={keepMoreSubmenu}
                    onpointerleave={scheduleCloseMoreSubmenu}
                    class="absolute top-[-4px] right-full z-50 w-[208px] pr-1.5"
                  >
                    <div class="lw-flyout-surface max-h-[320px] overflow-y-auto">
                      {#each LYRICS_ANIMATION_OPTIONS as anim (anim.id)}
                        <button
                          type="button"
                          onclick={() => {
                            featuresStore.setLyricsAnimation(anim.id);
                            moreMenuOpen = false;
                            moreSubmenu = null;
                          }}
                          class="lw-menu-row"
                        >
                          <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                            {#if featuresStore.lyricsAnimation === anim.id}<WaveIcon name="check" size={12} />{/if}
                          </span>
                          <span class="min-w-0 flex-1 truncate">{anim.label}</span>
                        </button>
                      {/each}
                    </div>
                  </div>
                {/if}
              </div>

              <div class="lw-menu-sep"></div>

              <button
                type="button"
                onpointerenter={scheduleCloseMoreSubmenu}
                onclick={toggleSelectMode}
                class="lw-menu-row"
              >
                <WaveIcon name="queue" size={13} />
                <span class="min-w-0 flex-1 truncate">
                  {selectMode ? 'Stop selecting lines' : 'Select lines'}
                </span>
              </button>

              <button
                type="button"
                onpointerenter={scheduleCloseMoreSubmenu}
                onclick={shareSongExcerpt}
                class="lw-menu-row"
              >
                <WaveIcon name="share" size={13} />
                <span class="min-w-0 flex-1 truncate">Share song card</span>
              </button>

              <button
                type="button"
                disabled={!selectMode || selectedIndices.length === 0}
                onpointerenter={scheduleCloseMoreSubmenu}
                onclick={shareSelectedLines}
                class="lw-menu-row"
              >
                <WaveIcon name="share" size={13} />
                <span class="min-w-0 flex-1 truncate">
                  {selectedIndices.length === 0
                    ? 'Share selected lines'
                    : `Share ${selectedIndices.length} lines`}
                </span>
              </button>
            </div>
          </div>
        {/if}

        <!-- Follow-scroll Pin / Unpin -->
        {#if hasLyrics && doc?.synced && !untimed}
          <button
            type="button"
            onclick={() => {
              if (!following) resumeFollowing();
              else following = false;
            }}
            use:waveTooltip={following ? 'Pause automatic scrolling' : 'Return to the line being sung'}
            class="flex h-[26px] w-[26px] cursor-pointer items-center justify-center rounded-[10px] transition hover:bg-[var(--lw-wash-hover)] {following ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]/90'}"
          >
            <WaveIcon name={following ? 'pin' : 'unpin'} size={13} />
          </button>
        {/if}

        {#if onClose}
          <button
            type="button"
            onclick={onClose}
            use:waveTooltip={'Close panel'}
            class="flex h-[26px] w-[26px] cursor-pointer items-center justify-center rounded-[10px] text-[var(--lw-text-primary)]/90 transition hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="close" size={13} />
          </button>
        {/if}
      </div>
    </div>
  {/if}

  <!-- 2. Multi-line Share Card Selection Banner (`_SelectBanner`) -->
  {#if selectMode}
    <div
      class="mx-2 mb-2 flex shrink-0 items-center gap-2 rounded-[10px] border border-[var(--lw-accent)]/50 bg-[var(--lw-accent-subtle)] px-3 py-1.5"
    >
      <WaveIcon name="checkList" size={13} class="shrink-0 text-[var(--lw-accent)]" />
      <span class="min-w-0 flex-1 truncate text-[12px] text-[var(--lw-text-secondary)]">
        {selectedIndices.length === 0
          ? 'Tap lines to include them on the card'
          : selectedIndices.length === 1
            ? '1 line selected'
            : `${selectedIndices.length} lines selected`}
      </span>
      <Buttons
        variant="primary"
        size="sm"
        disabled={selectedIndices.length === 0}
        onclick={shareSelectedLines}
      >
        Share
      </Buttons>
      <Buttons variant="secondary" size="sm" onclick={toggleSelectMode}>Cancel</Buttons>
    </div>
  {/if}

  <!-- 3. Lyrics Stage Body (Loading / Error / Instrumental / Empty / Karaoke Viewport) -->
  <div class="relative min-h-0 flex-1">
    {#if !track}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center px-6 text-center">
        <div class="text-[15px] font-semibold text-[var(--lw-text-primary)]">Nothing playing</div>
        <div class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">
          Play a track to view synced lyrics.
        </div>
      </div>
    {:else if !doc && featuresStore.lyricsLoading}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center gap-2.5 px-6 text-center">
        <div class="h-5 w-5 animate-spin rounded-full border-2 border-[var(--lw-outline)] border-t-[var(--lw-accent)]"></div>
        <div class="text-[13px] font-medium text-[var(--lw-text-secondary)]">Finding lyrics…</div>
      </div>
    {:else if !doc && errorMsg}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center gap-2 px-6 text-center">
        <div class="text-[15px] font-semibold text-[var(--lw-text-primary)]">Lyrics unavailable</div>
        <div class="text-[12.5px] text-[var(--lw-text-secondary)]">
          Check your connection and try again.
        </div>
        <div class="mt-2">
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true })}
          >
            Try again
          </Buttons>
        </div>
      </div>
    {:else if doc?.isInstrumental}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center gap-1.5 px-6 text-center">
        <WaveIcon name="music" size={24} class="mb-1 text-[var(--lw-text-tertiary)]" />
        <div class="text-[15px] font-semibold text-[var(--lw-text-primary)]">Instrumental</div>
        <div class="text-[12.5px] text-[var(--lw-text-secondary)]">No lyrics for this track.</div>
      </div>
    {:else if !doc || doc.lines.length === 0}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center gap-1.5 px-6 text-center">
        <WaveIcon name="artist" size={24} class="mb-1 text-[var(--lw-text-tertiary)]" />
        <div class="text-[15px] font-semibold text-[var(--lw-text-primary)]">No lyrics found</div>
        <div class="text-[12.5px] text-[var(--lw-text-secondary)]">
          Pick another source, or check back later.
        </div>
        <div class="mt-2 flex items-center gap-2">
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => featuresStore.selectTrackLyricsProvider(track, null)}
          >
            Try another source
          </Buttons>
          <Buttons
            variant="ghost"
            size="sm"
            onclick={() => void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true })}
          >
            Try again
          </Buttons>
        </div>
      </div>
    {:else}
      <!-- Masked Scrolling Karaoke Viewport (10% top / 10% bottom ShaderMask parity) -->
      <div
        bind:this={scrollEl}
        onwheel={handleUserWheel}
        ontouchmove={handleTouchMove}
        class="lw-karaoke-scroll h-full w-full overflow-y-auto"
        style:padding={compact ? '36px 12px 84px 12px' : '60px 24px 124px 24px'}
      >
        <div class="mx-auto flex w-full max-w-[740px] flex-col">
          {#each doc.lines as line, i (`${doc.trackId}-${i}-${line.startMs}`)}
            {@const isActive = !untimed && i === activeIdx}
            {@const rtl = isRtlText(line.text)}
            {@const joined = isJoinedScript(line.text)}
            {@const tracking = joined ? 0 : baseStyleTracking}
            {@const role = vocalRoles[i] ?? 'leadLeft'}
            {@const isDuetRight = role === 'duetRight'}
            {@const isUnison = role === 'unisonCenter'}
            {@const trimmedText = line.text.trim()}
            {@const isBackground =
              (line.words && line.words.length > 0 && line.words.every((w) => w.isBackground)) ||
              (trimmedText.startsWith('(') && trimmedText.endsWith(')'))}
            {@const effectiveSize = isBackground ? baseFontSize * 0.65 : baseFontSize}
            {@const subFontSize = Math.max(12, effectiveSize * 0.52)}
            {@const prevEndMs = prevEndForBreak(doc.lines, i)}
            {@const gapMs = line.startMs - prevEndMs}
            {@const hasBreak = !untimed && (i === 0 ? line.startMs >= 2500 : gapMs >= 3000)}
            {@const breakActive = hasBreak && posMs >= prevEndMs && posMs < line.startMs}
            {@const lineAlpha = distanceOpacity(i, activeIdx)}
            {@const selected = selectedIndices.includes(i)}
            {@const canTap = selectMode || (!untimed && doc.synced)}

            <div
              data-lw="lyrics-line"
              data-lyric-idx={i}
              data-active={isActive}
              class="mb-1 flex flex-col {isDuetRight
                ? 'items-end'
                : isUnison
                  ? 'items-center'
                  : 'items-start'}"
            >
              <!-- Glowing `♪ • • •` Instrumental Countdown Dots -->
              {#if breakActive}
                {@const breakProg = Math.max(0, Math.min(1, (posMs - prevEndMs) / Math.max(1, line.startMs - prevEndMs)))}
                {@const dotBase = effectiveSize * 0.22}
                <div
                  aria-label="Instrumental break"
                  class="flex items-center py-2.5 {compact ? 'px-2' : 'px-3'}"
                  style:gap={`${(effectiveSize * 0.18).toFixed(1)}px`}
                >
                  <span
                    class="font-bold text-[var(--lw-lyric-ink)]"
                    style:font-size={`${effectiveSize.toFixed(1)}px`}
                    style:line-height="1.1"
                    style:margin-right={`${(effectiveSize * 0.17).toFixed(1)}px`}
                    style:text-shadow={`0 0 ${(8 + 8 * breakPulse).toFixed(1)}px color-mix(in srgb, var(--lw-lyric-ink) ${Math.round((0.55 + 0.35 * breakPulse) * 100)}%, transparent)`}
                  >♪</span>
                  {#each [0, 1, 2] as dotIdx (dotIdx)}
                    {@const isLit = breakProg >= (dotIdx + 1) / 4}
                    {@const dSize = isLit ? dotBase * 1.15 : dotBase * 0.85}
                    <span
                      class="inline-block rounded-full transition-all duration-150"
                      style:width={`${dSize.toFixed(1)}px`}
                      style:height={`${dSize.toFixed(1)}px`}
                      style:background={isLit ? 'var(--lw-lyric-ink)' : 'color-mix(in srgb, var(--lw-lyric-ink) 28%, transparent)'}
                      style:box-shadow={isLit
                        ? `0 0 ${(6 + 4 * breakPulse).toFixed(1)}px color-mix(in srgb, var(--lw-lyric-ink) ${Math.round((0.65 + 0.35 * breakPulse) * 100)}%, transparent)`
                        : 'none'}
                    ></span>
                  {/each}
                </div>
              {/if}

              <!-- Tappable Lyric Block -->
              <div
                class="max-w-full rounded-[12px] transition-colors duration-150 {selected
                  ? 'bg-[var(--lw-accent)]/16'
                  : ''}"
                style:max-width={isUnison || compact ? '100%' : '78%'}
              >
                <button
                  type="button"
                  disabled={!canTap}
                  onclick={() => seekToLine(line, i)}
                  dir={rtl ? 'rtl' : 'ltr'}
                  class="lw-lyric-btn group block w-full rounded-[10px] {canTap
                    ? 'cursor-pointer'
                    : 'cursor-default'} {isDuetRight
                    ? 'text-right'
                    : isUnison
                      ? 'text-center'
                      : 'text-left'}"
                  style:padding={compact ? '6px 8px' : isWordKaraoke ? '6px 12px' : '8px 12px'}
                  style:padding-right={!rtl ? (compact ? '16px' : '26px') : undefined}
                  style:padding-left={rtl ? (compact ? '16px' : '26px') : undefined}
                  style:opacity={lineAlpha}
                  style:transform={isActive ? activeLineTransform(rtl) : 'none'}
                  style:transform-origin={rtl || isDuetRight
                    ? 'right center'
                    : isUnison
                      ? 'center center'
                      : 'left center'}
                  style:transition={`transform ${lineTransitionDuration} ${lineTransitionCurve}, opacity 220ms cubic-bezier(0.22, 1, 0.36, 1), background-color 150ms ease`}
                >
                  <!-- Main Sung Text / Progressive Word-by-Word Karaoke Wipe -->
                  <div
                    class="lw-lyric-main"
                    style:font-size={`${effectiveSize.toFixed(1)}px`}
                    style:font-weight={lineFontWeight}
                    style:font-style={lineFontStyle}
                    style:line-height="1.42"
                    style:letter-spacing={`${tracking}px`}
                  >
                    {#if isActive && isWordKaraoke}
                      {@const wordGroups = lineWordsForKaraoke(line)}
                      <span
                        class="inline-flex flex-wrap items-center"
                        class:justify-end={isDuetRight}
                        class:justify-center={isUnison}
                      >
                        {#each wordGroups as wg, wgi (wgi)}
                          <span class="inline-flex items-center whitespace-pre">
                            {#each wg.syllables as syl, si (`${syl.startMs}-${si}`)}
                              <KaraokeSyllable
                                syllable={syl}
                                {posMs}
                                isRtl={rtl}
                                animationStyle={animStyle}
                                {activeShadow}
                              />
                            {/each}{#if wg.hasTrailingSpace}<span class="inline-block text-[var(--lw-lyric-ink)]" style:opacity={0.38}>&nbsp;</span>{/if}
                          </span>
                        {/each}
                      </span>
                    {:else}
                      <span
                        class="text-[var(--lw-lyric-ink)] transition-opacity duration-180"
                        style:opacity={isActive ? 1 : untimed ? 0.92 : 0.44}
                        style:text-shadow={isActive ? activeShadow : 'none'}
                      >{line.text}</span>
                    {/if}
                  </div>

                  <!-- Inline Translation (`baseFontSize * 0.52`, LTR, alpha 0.78 active / 0.42 idle) -->
                  {#if featuresStore.showTranslation && line.translation}
                    <div
                      dir="ltr"
                      class="mt-1 font-normal text-[var(--lw-lyric-ink)] transition-opacity duration-200"
                      style:font-size={`${subFontSize.toFixed(1)}px`}
                      style:line-height="1.32"
                      style:opacity={isActive ? 0.78 : 0.42}
                    >
                      {line.translation}
                    </div>
                  {/if}

                  <!-- Inline Romanisation (`baseFontSize * 0.52`, italic, alpha 0.72 active / 0.40 idle) -->
                  {#if featuresStore.showRomanized && line.romanized}
                    <div
                      dir={rtl ? 'rtl' : 'ltr'}
                      class="mt-1 font-normal italic text-[var(--lw-lyric-ink)] transition-opacity duration-200"
                      style:font-size={`${subFontSize.toFixed(1)}px`}
                      style:line-height="1.32"
                      style:opacity={isActive ? 0.72 : 0.40}
                    >
                      {line.romanized}
                    </div>
                  {/if}
                </button>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- 4. Floating Frosted `Return to current` Pill (`_ReturnToCurrentPill`) -->
      {#if !following && doc.synced && !untimed}
        <button
          type="button"
          onclick={resumeFollowing}
          class="lw-flyout-surface absolute right-4 bottom-4 z-30 flex cursor-pointer items-center gap-1.5 rounded-[18px] px-3.5 py-2 text-[12px] font-semibold text-[var(--lw-text-primary)] shadow-[var(--lw-shadow-flyout)] transition hover:bg-[var(--lw-surface-overlay)]"
        >
          <WaveIcon name="down" size={12} class="text-[var(--lw-accent)]" />
          <span>Return to current</span>
        </button>
      {/if}
    {/if}
  </div>
</div>

<!-- 1080×1080 PNG Lyrics Share Card Dialog -->
<LyricsShareCard
  open={shareLines !== null}
  lines={shareLines ?? []}
  onClose={() => (shareLines = null)}
/>

<style>
  .lw-karaoke-root {
    --lw-lyric-ink: var(--lw-text-primary);
  }
  .lw-karaoke-scroll {
    scrollbar-width: none;
    -webkit-mask-image: linear-gradient(
      to bottom,
      transparent 0%,
      #000 10%,
      #000 90%,
      transparent 100%
    );
    mask-image: linear-gradient(
      to bottom,
      transparent 0%,
      #000 10%,
      #000 90%,
      transparent 100%
    );
  }
  .lw-karaoke-scroll::-webkit-scrollbar {
    display: none;
    width: 0;
    height: 0;
  }
  @media (hover: hover) and (pointer: fine) {
    .lw-lyric-btn:not(:disabled):hover {
      background-color: var(--lw-wash-hover);
    }
    .lw-lyric-btn:not(:disabled):hover .lw-lyric-main > span {
      opacity: 0.9 !important;
    }
  }
</style>
