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
    preserveAspect?: boolean;
    highResolution?: boolean;
    size?: number;
    radius?: number;
    circle?: boolean;
    shadow?: 'none' | 'lift' | 'artwork';
    class?: string;
    onArtworkLoad?: (url: string) => void;
  }

  // Session cache of studio-upgraded URLs & seen URLs so tiles swap instantaneously.
  const upgradedCache = new Map<string, string>();
  const shownUrls = new Set<string>();
  const UPGRADED_CAP = 500;

  function bucketPx(target: number): number {
    if (target <= 96) return 96;
    if (target <= 160) return 160;
    if (target <= 320) return 320;
    if (target <= 600) return 600;
    return 1024;
  }

  function sizedUrl(raw: string, displaySize?: number): string {
    const url = raw.trim();
    if (!url) return '';
    const px = bucketPx((displaySize ?? 160) * 2);
    if (url.includes('mzstatic.com') || url.includes('apple.com')) {
      return url
        .replace(/\d+x\d+bb/, `${px}x${px}bb`)
        .replace('{w}', String(px))
        .replace('{h}', String(px))
        .replace('{f}', 'jpg');
    }
    if (url.includes('googleusercontent.com') || url.includes('ggpht.com')) {
      const next = url.replace(/=w\d+(-h\d+)?(-c)?(-p)?(-l\d+)?(-rj)?/, `=w${px}-h${px}-l90-rj`);
      if (next !== url) return next;
      const slash = url.replace(/\/w\d+(-h\d+)?\//, `/w${px}-h${px}/`);
      if (slash !== url) return slash;
    }
    if (url.includes('ytimg.com') && px < 300 && url.includes('hqdefault')) {
      return url.replace('hqdefault', 'mqdefault');
    }
    return url;
  }

  let {
    src = '',
    alt = '',
    label = '',
    kind = 'track',
    artist = '',
    album = '',
    upgrade = true,
    preserveAspect = false,
    highResolution = false,
    size,
    radius = 12,
    circle = false,
    shadow = 'none',
    class: className = '',
    onArtworkLoad,
  }: Props = $props();

  let failedPrimary = $state(false);
  let failedLarger = $state(false);
  let failedResolved = $state(false);
  let loaded = $state(false);
  let resolvedUrl = $state('');
  let largerUrl = $derived(highResolution ? largerArtworkUrl(src) : '');
  let imgEl = $state<HTMLImageElement | null>(null);
  let lastActiveUrl = '';
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
      lower.includes('scdn.co') ||
      lower.includes('qobuz.com') ||
      lower.includes('resources.tidal.com') ||
      lower.includes('lastfm.freetls.fastly.net') ||
      lower.includes('lh3.googleusercontent.com') ||
      lower.includes('yt3.googleusercontent.com') ||
      lower.includes('yt3.ggpht.com') ||
      lower.startsWith('asset:') ||
      lower.startsWith('data:')
    );
  }

  function largerArtworkUrl(src: string): string {
    let url: URL;
    try { url = new URL(src); } catch { return ''; }
    if (url.protocol !== 'https:') return '';
    const host = url.hostname;
    const originalPath = url.pathname;
    const target = 1600;
    const enlarge = (width: string, height = width) => Math.min(Number(width), Number(height)) < target;
    if (/\.(?:googleusercontent|ggpht)\.com$/.test(host)) {
      url.pathname = originalPath.replace(/=w(\d+)-h(\d+)(?:-[^/]*)?$/, (size, width, height) =>
        enlarge(width, height) ? `=w${target}-h${target}-l90-rj` : size);
      if (url.pathname === originalPath) {
        url.pathname = originalPath
          .replace(/=s(\d+)(?:-[^/]*)?$/, (size, dimension) =>
            enlarge(dimension) ? `=s${target}` : size)
          .replace(/\/w(\d+)-h(\d+)\//, (size, width, height) =>
            enlarge(width, height) ? `/w${target}-h${target}/` : size);
      }
    } else if (host.endsWith('.mzstatic.com')) {
      url.pathname = originalPath.replace(/\/(\d+)x(\d+)([a-z]*)(?=\.)/i, (size, width, height, format) =>
        enlarge(width, height) ? `/${target}x${target}${format}` : size);
    } else if (host.endsWith('.ytimg.com')) {
      url.pathname = originalPath.replace(/\/(?:default|mqdefault|hqdefault|sddefault)(?=\.)/, '/maxresdefault');
    }
    return url.pathname === originalPath ? '' : url.href;
  }

  $effect(() => {
    const currentSrc = src;
    const currentKind = kind;
    const currentLabel = label || alt;
    const currentArtist = artist;
    const currentAlbum = album;
    const verifiedCover = highResolution && currentKind === 'track' && Boolean(currentArtist);
    const seq = ++resolveSeq;

    failedPrimary = false;
    failedLarger = false;
    failedResolved = false;
    resolvedUrl = '';

    if (!upgrade || !isTauriRuntime() || !currentLabel || (!verifiedCover && isOfficialUrl(currentSrc))) {
      return;
    }

    const cacheKey = `${verifiedCover ? 'verified-track' : currentKind}|${currentLabel.toLowerCase()}|${currentArtist.toLowerCase()}|${currentAlbum.toLowerCase()}`;
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
              verified: verifiedCover || undefined,
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
        if (url && !failedResolved) {
          const sized = sizedUrl(url, size);
          const preloader = new Image();
          preloader.onload = () => {
            if (seq === resolveSeq && !failedResolved) resolvedUrl = sized;
          };
          preloader.src = sized;
        }
      })
      .catch(() => {});
  });

  let activeUrl = $derived.by(() => {
    if (resolvedUrl && !failedResolved) return sizedUrl(resolvedUrl, size);
    if (largerUrl && !failedLarger) return largerUrl;
    if (src && !failedPrimary) return sizedUrl(src, size);
    return '';
  });

  $effect(() => {
    const url = activeUrl;
    if (url !== lastActiveUrl) {
      lastActiveUrl = url;
      loaded = Boolean(url && (shownUrls.has(url) || (imgEl?.complete && imgEl.naturalWidth > 0)));
    } else if (url && imgEl?.complete && imgEl.naturalWidth > 0) {
      loaded = true;
    }
  });

  let skipFade = $derived((size ?? 64) <= 56 || Boolean(resolvedUrl) || (activeUrl ? shownUrls.has(activeUrl) : false));

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
    } else if (largerUrl && activeUrl === largerUrl) {
      failedLarger = true;
    } else {
      failedPrimary = true;
    }
  }
</script>

<div
  data-lw="artwork"
  class="relative shrink-0 overflow-hidden bg-[var(--lw-surface-overlay)] {className}"
  style:width={preserveAspect ? (activeUrl && loaded ? 'auto' : size ? `${size}px` : undefined) : size ? `${size}px` : undefined}
  style:height={size ? `${size}px` : undefined}
  style:border-radius={circle ? '999px' : `${radius}px`}
  style:box-shadow={shadowStyle}
>
  {#if activeUrl}
    <img
      bind:this={imgEl}
      src={activeUrl}
      {alt}
      loading={highResolution ? 'eager' : undefined}
      decoding="async"
      onload={(event) => {
        loaded = true;
        if (activeUrl) {
          if (shownUrls.size >= UPGRADED_CAP) {
            const first = shownUrls.values().next().value;
            if (first !== undefined) shownUrls.delete(first);
          }
          shownUrls.add(activeUrl);
        }
        onArtworkLoad?.((event.currentTarget as HTMLImageElement).currentSrc);
      }}
      onerror={handleError}
      class={preserveAspect ? 'block h-full w-auto max-w-none' : 'h-full w-full object-cover'}
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
