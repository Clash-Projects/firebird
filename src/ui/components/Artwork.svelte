<script lang="ts">
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    src?: string;
    alt?: string;
    label?: string;
    kind?: 'track' | 'album' | 'artist';
    artist?: string;
    album?: string;
    upgrade?: boolean;
    size?: number;
    radius?: number;
    circle?: boolean;
    shadow?: 'none' | 'lift' | 'artwork';
    class?: string;
  }

  // Session cache of studio-upgraded URLs so tiles across routes swap instantaneously.
  const upgradedCache = new Map<string, string>();
  const UPGRADED_CAP = 500;

  let {
    src = '',
    alt = '',
    label = '',
    kind = 'track',
    artist = '',
    album = '',
    upgrade = true,
    size,
    radius = 12,
    circle = false,
    shadow = 'none',
    class: className = '',
  }: Props = $props();

  let failedPrimary = $state(false);
  let failedResolved = $state(false);
  let loaded = $state(false);
  let resolvedUrl = $state('');
  let resolveSeq = 0;

  let displayLabel = $derived((label || album || alt || artist || '').trim());

  let initials = $derived.by(() => {
    const parts = displayLabel.split(/\s+/).filter(Boolean);
    if (parts.length === 0) return '';
    if (parts.length === 1) {
      return parts[0].slice(0, 2).toUpperCase();
    }
    return `${parts[0][0] ?? ''}${parts[1][0] ?? ''}`.toUpperCase();
  });

  // Deterministic low-saturation duo-tone hue matching OG Artwork (_fallback).
  let fallbackHue = $derived.by(() => {
    if (!displayLabel) return 215;
    let hash = 0;
    for (let i = 0; i < displayLabel.length; i++) {
      hash = (hash * 31 + displayLabel.charCodeAt(i)) | 0;
    }
    return Math.abs(hash) % 360;
  });

  function isOfficialUrl(url: string): boolean {
    const lower = url.toLowerCase();
    return (
      lower.includes('mzstatic.com') ||
      lower.includes('dzcdn.net') ||
      lower.includes('lastfm.freetls.fastly.net') ||
      lower.startsWith('asset:') ||
      lower.startsWith('data:')
    );
  }

  $effect(() => {
    const currentSrc = src;
    const currentKind = kind;
    const currentLabel = label || alt;
    const currentArtist = artist;
    const currentAlbum = album;
    const seq = ++resolveSeq;

    failedPrimary = false;
    failedResolved = false;
    loaded = false;
    resolvedUrl = '';

    if (!upgrade || !isTauriRuntime() || !currentLabel || isOfficialUrl(currentSrc)) {
      return;
    }

    const cacheKey = `${currentKind}|${currentLabel.toLowerCase()}|${currentArtist.toLowerCase()}|${currentAlbum.toLowerCase()}`;
    const cached = upgradedCache.get(cacheKey);
    if (cached !== undefined) {
      resolvedUrl = cached;
      return;
    }

    const req =
      currentKind === 'artist'
        ? ipcInvoke<{ artworkUrl?: string } | null>('artwork_resolve_artist', { artist: currentLabel })
        : currentKind === 'album'
          ? ipcInvoke<{ artworkUrl?: string } | null>('artwork_resolve_album', {
              album: currentAlbum || currentLabel,
              artist: currentArtist,
            })
          : ipcInvoke<{ artworkUrl?: string } | null>('artwork_resolve_track', {
              title: currentLabel,
              artist: currentArtist,
              album: currentAlbum || null,
            });

    req
      .then((res) => {
        if (seq !== resolveSeq) return;
        const url = res?.artworkUrl?.trim() ?? '';
        if (upgradedCache.size >= UPGRADED_CAP) {
          const oldest = upgradedCache.keys().next().value;
          if (oldest !== undefined) upgradedCache.delete(oldest);
        }
        upgradedCache.set(cacheKey, url);
        if (url && !failedResolved) resolvedUrl = url;
      })
      .catch(() => {});
  });

  let activeUrl = $derived.by(() => {
    if (resolvedUrl && !failedResolved) return resolvedUrl;
    if (src && !failedPrimary) return src;
    return '';
  });

  let skipFade = $derived((size ?? 64) <= 56 || Boolean(resolvedUrl));

  let shadowStyle = $derived(
    shadow === 'artwork'
      ? 'var(--lw-shadow-artwork)'
      : shadow === 'lift'
        ? 'var(--lw-shadow-lift)'
        : undefined
  );

  function handleError() {
    if (resolvedUrl && !failedResolved && activeUrl === resolvedUrl) {
      failedResolved = true;
    } else {
      failedPrimary = true;
    }
  }
</script>

<div
  data-lw="artwork"
  class="relative shrink-0 overflow-hidden bg-[var(--lw-surface-overlay)] {className}"
  style:width={size ? `${size}px` : undefined}
  style:height={size ? `${size}px` : undefined}
  style:border-radius={circle ? '999px' : `${radius}px`}
  style:box-shadow={shadowStyle}
>
  {#if activeUrl}
    <img
      src={activeUrl}
      {alt}
      loading="lazy"
      decoding="async"
      onload={() => (loaded = true)}
      onerror={handleError}
      class="h-full w-full object-cover"
      style:opacity={skipFade || loaded ? 1 : 0}
      style:transition={skipFade ? 'none' : 'opacity var(--lw-motion-fast) var(--ease-native)'}
    />
  {:else}
    <div
      class="art-fallback flex h-full w-full flex-col items-center justify-center select-none"
      style:--art-hue={fallbackHue}
    >
      {#if initials}
        <span
          class="font-bold leading-none tracking-[-0.02em]"
          style:font-size="{size ? Math.max(10, Math.round(size * 0.24)) : 16}px"
        >
          {initials}
        </span>
      {:else}
        <WaveIcon name="music" size={size ? Math.max(14, Math.round(size * 0.3)) : 24} />
      {/if}
      <span
        class="art-fallback-bar rounded-[2px]"
        style:margin-top="{size ? Math.max(2, Math.round(size * 0.06)) : 4}px"
        style:width="{size ? Math.max(10, Math.round(size * 0.22)) : 16}px"
      ></span>
    </div>
  {/if}
</div>

<style>
  .art-fallback {
    background: linear-gradient(
      135deg,
      hsl(var(--art-hue) 22% 22%),
      hsl(calc(var(--art-hue) + 28) 26% 14%)
    );
    color: rgba(255, 255, 255, 0.82);
  }

  .art-fallback-bar {
    height: 2px;
    background: currentColor;
    opacity: 0.42;
  }

  :global(:root[data-theme='light']) .art-fallback {
    background: linear-gradient(
      135deg,
      hsl(var(--art-hue) 28% 82%),
      hsl(calc(var(--art-hue) + 28) 30% 72%)
    );
    color: rgba(0, 0, 0, 0.62);
  }
</style>
