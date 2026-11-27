<script lang="ts">
  import { onMount } from 'svelte';
  import { router } from '$src/app/router.svelte';
  import KaraokeLyricsView from '$src/features/lyrics/KaraokeLyricsView.svelte';
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import AmbientStage from '$src/ui/components/AmbientStage.svelte';
  import AnimatedArtworkVideo from '$src/ui/components/AnimatedArtworkVideo.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { immersiveState } from './immersive.svelte';
  import NowPlayingFullBleedView from './NowPlayingFullBleedView.svelte';
  import NowPlayingMinimalView from './NowPlayingMinimalView.svelte';
  import NowPlayingOverflowMenu from './NowPlayingOverflowMenu.svelte';

  let track = $derived(playerStore.currentTrack);
  let nextTrack = $derived(playerStore.queue[playerStore.currentIndex + 1] ?? null);
  let doc = $derived(track ? featuresStore.getLyrics(track.id) : null);
  let lyricsMissing = $derived(!featuresStore.lyricsLoading && (!doc || doc.lines.length === 0));

  let lyricsVisible = $state(true);
  let focusMode = $state(false);
  let ambientEnabled = $state(true);
  let loadedCover = $state<{ trackId: string; url: string } | null>(null);
  let ambientArtworkUrl = $derived(
    track && loadedCover?.trackId === track.id ? loadedCover.url : track?.artworkUrl ?? ''
  );
  let albumMotion = $state<{ trackId: string; urls: string[] } | null>(null);
  let spotifyMotion = $state<{ trackId: string; url?: string } | null>(null);
  let albumVideoUrls = $derived(albumMotion && albumMotion.trackId === track?.id ? albumMotion.urls : []);
  let fullBleedVideoUrls = $derived(
    spotifyMotion?.url && spotifyMotion.trackId === track?.id
      ? [spotifyMotion.url, ...albumVideoUrls]
      : albumVideoUrls,
  );

  let effectiveLyricsVisible = $derived(lyricsVisible && !lyricsMissing);
  let effectiveFocusMode = $derived(focusMode && !lyricsMissing);

  $effect(() => {
    if (!track) return;
    const { id, title, artist, album } = track;
    let current = true;
    void ipcInvoke<{ url: string; fallbackUrl?: string | null }>('artwork_lookup_animated', {
      artist, album: album || '', title,
    }).then((artwork) => {
      if (!current) return;
      const urls = [artwork?.url, artwork?.fallbackUrl].filter((url): url is string => Boolean(url));
      albumMotion = { trackId: id, urls };
    });
    return () => { current = false; };
  });

  $effect(() => {
    if (!track || settingsStore.nowPlayingStyle !== 'fullBleed') return;
    const { id, title, artist, album, durationSec } = track;
    let current = true;
    void ipcInvoke<{ clip?: { url: string } | null }>('spotify_canvas', {
      title, artist, album: album || null, durationMs: Math.round((durationSec || 0) * 1000),
    }).then((canvas) => {
      if (current) spotifyMotion = { trackId: id, url: canvas?.clip?.url };
    });
    return () => { current = false; };
  });

  function close() {
    if (immersiveState.isImmersive) {
      void immersiveState.setImmersive(false);
      return;
    }
    if (router.canGoBack) router.back();
    else router.navigate('/home');
  }

  function toggleLyrics() {
    immersiveState.ping();
    lyricsVisible = !lyricsVisible;
    if (!lyricsVisible) focusMode = false;
  }

  function toggleFocus() {
    immersiveState.ping();
    focusMode = !focusMode;
  }

  function rememberCover(url: string) {
    if (track) loadedCover = { trackId: track.id, url };
  }

  onMount(() => {
    immersiveState.ping();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'F11') {
        e.preventDefault();
        immersiveState.toggleImmersive();
      } else if (e.key === 'Escape') {
        close();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      immersiveState.reset();
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  data-lw="now-playing"
  class="now-playing"
  style:cursor={immersiveState.controlsIdle ? 'none' : 'default'}
  onpointermove={() => immersiveState.ping()}
  onpointerdown={() => immersiveState.ping()}
>
  {#if !track}
    <div class="now-empty">
      <span class="now-empty-icon"><WaveIcon name="album" size={32} /></span>
      <h1>Nothing playing</h1>
      <p>Pick something from Home, Discover or Search and it will take centre stage here.</p>
      <button type="button" onclick={() => router.navigate('/home')}>
        <WaveIcon name="search" size={16} /> Browse music
      </button>
    </div>
  {:else if settingsStore.nowPlayingStyle === 'fullBleed'}
    <NowPlayingFullBleedView videoUrls={fullBleedVideoUrls} {ambientArtworkUrl} onArtworkLoad={rememberCover} />
  {:else if settingsStore.nowPlayingStyle === 'minimal'}
    <NowPlayingMinimalView
      {lyricsVisible}
      videoUrls={albumVideoUrls}
      {ambientArtworkUrl}
      onArtworkLoad={rememberCover}
      onToggleLyrics={toggleLyrics}
      onClose={close}
    />
  {:else}
    {#if ambientEnabled}
      <AmbientStage artworkUrl={ambientArtworkUrl} intensity={0.32} />
    {/if}

    <header
      class="now-header transition-opacity duration-350"
      class:pointer-events-none={immersiveState.controlsIdle}
      style:opacity={immersiveState.controlsIdle ? 0 : 1}
    >
      <div class="now-label"><WaveIcon name="headphones" size={17} /> Now playing</div>
      <div class="now-actions">
        <button
          type="button"
          aria-pressed={lyricsVisible}
          onclick={toggleLyrics}
        >
          <WaveIcon name="lyrics" size={15} /> Lyrics
        </button>
        {#if effectiveLyricsVisible}
          <button type="button" aria-pressed={focusMode} onclick={toggleFocus}>Focus lyrics</button>
        {/if}
        <NowPlayingOverflowMenu
          showAmbientToggle={true}
          {ambientEnabled}
          onToggleAmbient={() => (ambientEnabled = !ambientEnabled)}
          {lyricsVisible}
          onToggleLyrics={toggleLyrics}
          {focusMode}
          onToggleFocus={toggleFocus}
        />
        <button type="button" onclick={close} aria-label="Close Now Playing" use:waveTooltip={'Close Now Playing'}>
          <WaveIcon name="close" size={16} />
        </button>
      </div>
    </header>

    <div class="now-body">
      <div class="now-stage" class:with-lyrics={effectiveLyricsVisible && !effectiveFocusMode}>
        {#if !effectiveFocusMode}
          {#key track.id}
            <div class="now-track lw-enter" style:--lw-rise="9px">
              <div class="now-cover">
                <AnimatedArtworkVideo
                  posterUrl={track.artworkUrl}
                  title={track.title}
                  artist={track.artist}
                  album={track.album}
                  onArtworkLoad={rememberCover}
                  videoUrls={ambientEnabled ? albumVideoUrls : []}
                  radius={18}
                  class="aspect-square w-full"
                />
              </div>
              <div class="now-meta">
                <h1>{track.title}</h1>
                <p>{track.artist}</p>
                <div class="now-format lw-tabular">
                  <WaveIcon name="activity" size={13} />
                  {track.codec} · {track.bitDepth}-bit / {(track.sampleRateHz / 1000).toFixed(0)} kHz
                </div>
                {#if nextTrack}
                  <p class="now-next">
                    <strong>Up next</strong> <span>{nextTrack.title} · {nextTrack.artist}</span>
                  </p>
                {/if}
              </div>
            </div>
          {/key}
        {/if}

        {#if effectiveLyricsVisible}
          <div use:waveSmoothScroll class="now-lyrics lw-scroll lw-route-enter">
            <KaraokeLyricsView fontSize={32} />
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .now-playing {
    position: relative;
    display: flex;
    height: 100%;
    flex-direction: column;
    overflow: hidden;
    background: var(--lw-bg);
  }

  .now-header { position: relative; z-index: 20; display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 17px clamp(18px, 3vw, 39px) 0; }
  .now-label { display: flex; align-items: center; gap: 8px; color: var(--lw-text-secondary); font-size: 12px; font-weight: 700; }
  .now-actions { display: flex; align-items: center; gap: 6px; }
  .now-actions > button {
    display: inline-flex;
    min-height: 32px;
    align-items: center;
    gap: 6px;
    padding-inline: 9px;
    border: 1px solid transparent;
    border-radius: 6px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    font-size: 12px;
    font-weight: 650;
    transition: background-color var(--lw-motion-fast), color var(--lw-motion-fast);
  }
  .now-actions > button[aria-pressed="true"] { border-color: var(--lw-outline); background: var(--lw-surface); color: var(--lw-accent); }
  .now-body { position: relative; z-index: 10; display: grid; min-height: 0; flex: 1; }
  .now-stage {
    display: grid;
    width: min(100%, 1250px);
    min-height: 0;
    grid-template-columns: minmax(0, 1fr);
    place-items: center;
    gap: clamp(24px, 5.6vw, 88px);
    margin-inline: auto;
    padding: 13px clamp(24px, 5vw, 82px) 24px;
  }
  .now-stage.with-lyrics { grid-template-columns: minmax(0, 0.82fr) minmax(0, 1.18fr); }
  .now-track { width: min(100%, clamp(152px, 45vh, 490px)); min-width: 0; }
  .now-cover { overflow: hidden; border-radius: 18px; box-shadow: var(--lw-shadow-artwork); }
  .now-meta { margin-top: 13px; }
  .now-meta h1 {
    display: -webkit-box;
    overflow: hidden;
    font-size: clamp(24px, 3.2vw, 38px);
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.08;
    overflow-wrap: anywhere;
    text-wrap: balance;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .now-meta p { overflow: hidden; margin-top: 4px; color: var(--lw-text-secondary); font-size: 14px; text-overflow: ellipsis; white-space: nowrap; }
  .now-format { display: inline-flex; align-items: center; gap: 5px; margin-top: 9px; padding: 3px 7px; border: 1px solid var(--lw-outline); border-radius: 6px; background: var(--lw-surface); color: var(--lw-accent); font-size: 10.5px; font-weight: 600; }
  .now-meta .now-next { display: flex; align-items: baseline; gap: 7px; margin-top: 10px; font-size: 11.5px; }
  .now-next strong { flex: none; color: var(--lw-text-primary); }
  .now-next span { overflow: hidden; text-overflow: ellipsis; }
  .now-lyrics { width: min(100%, 660px); height: 100%; min-height: 0; }
  .now-empty { position: relative; align-self: center; justify-self: center; max-width: 90%; padding: 22px 28px 50px; text-align: center; margin: auto; }
  .now-empty-icon { display: grid; width: 62px; height: 62px; place-items: center; margin: 0 auto 13px; border: 1px solid var(--lw-outline); border-radius: 16px; background: var(--lw-surface-raised); color: var(--lw-accent); }
  .now-empty h1 { font-size: clamp(29px, 4vw, 46px); font-weight: 700; }
  .now-empty p { color: var(--lw-text-secondary); font-size: 14px; }
  .now-empty button { display: inline-flex; min-height: 38px; align-items: center; gap: 7px; margin-top: 17px; padding-inline: 13px; border-radius: 9px; background: var(--lw-accent); color: var(--lw-on-accent); cursor: pointer; font-weight: 700; }

  @media (hover: hover) and (pointer: fine) {
    .now-actions > button:hover { background: var(--lw-wash-hover); color: var(--lw-text-primary); }
  }

  @media (max-height: 650px) {
    .now-track { width: min(100%, 38vh); }
    .now-meta { margin-top: 9px; }
  }
</style>
