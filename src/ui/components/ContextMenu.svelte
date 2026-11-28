<script lang="ts">
  import { tick } from 'svelte';
  import { clampSubmenu, dismissDialogOnBackdrop } from '$src/app/accessibility';
  import {
    downloadTrack,
    enqueueTrack,
    playTrackNext,
    playTrackNow,
    toggleTrackLike,
  } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import type { Playlist, Track } from '$src/state/types';
  import PlaylistDialogs from '$src/ui/collections/PlaylistDialogs.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import { goAlbum, goArtistByName } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { contextMenu } from './contextMenuState.svelte';

  let track = $derived(contextMenu.track);
  let menu = $state<HTMLDivElement>();
  let submenuOpen = $state(false);
  let submenuFlipLeft = $state(false);
  let submenuOpenUp = $state(false);
  let previousFocus: HTMLElement | null = null;
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);
  let customPlaylists = $derived(
    libraryStore.playlists.filter((p) => p.id !== 'pl-liked' && p.source === 'local')
  );

  let propsDialog = $state<HTMLDialogElement | null>(null);
  let pendingTrackForPlaylist = $state<Track | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let closeSubmenuTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (!contextMenu.open || !menu) {
      submenuOpen = false;
      return;
    }
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && !menu.contains(focused)) previousFocus = focused;
    void tick().then(() => {
      if (!menu || !contextMenu.open) return;
      const rect = menu.getBoundingClientRect();
      const scale =
        menu.offsetWidth > 0 && rect.width > 0 ? rect.width / menu.offsetWidth : 1;
      const vw = window.innerWidth / scale;
      const vh = window.innerHeight / scale;
      contextMenu.clampToViewport(rect.width / scale, rect.height / scale);
      submenuFlipLeft = contextMenu.x + rect.width / scale + 236 > vw;
      submenuOpenUp = contextMenu.y + rect.height / scale * 0.5 + 220 > vh;
      menu.querySelector<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)')?.focus({ preventScroll: true });
    });
  });

  $effect(() => {
    if (!propsDialog) return;
    if (contextMenu.propertiesTrack && !propsDialog.open) {
      propsDialog.showModal();
    } else if (!contextMenu.propertiesTrack && propsDialog.open) {
      propsDialog.close();
    }
  });

  function cancelSubmenuTimers() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    if (closeSubmenuTimer) {
      clearTimeout(closeSubmenuTimer);
      closeSubmenuTimer = null;
    }
  }

  function hoverSubmenuRow() {
    if (closeSubmenuTimer) {
      clearTimeout(closeSubmenuTimer);
      closeSubmenuTimer = null;
    }
    if (submenuOpen) return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      submenuOpen = true;
    }, 180);
  }

  function openSubmenuNow() {
    cancelSubmenuTimers();
    submenuOpen = true;
  }

  function scheduleCloseSubmenu() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    if (!submenuOpen) return;
    if (closeSubmenuTimer) clearTimeout(closeSubmenuTimer);
    closeSubmenuTimer = setTimeout(() => {
      submenuOpen = false;
    }, 320);
  }

  function onMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (submenuOpen) {
        submenuOpen = false;
        return;
      }
      contextMenu.close();
      previousFocus?.focus({ preventScroll: true });
      return;
    }

    if (event.key === 'ArrowRight' && document.activeElement?.getAttribute('data-submenu-trigger') === 'true') {
      event.preventDefault();
      openSubmenuNow();
      void tick().then(() => {
        menu?.querySelector<HTMLButtonElement>('[data-submenu-panel] button[role="menuitem"]')?.focus();
      });
      return;
    }

    if (event.key === 'ArrowLeft' && submenuOpen) {
      event.preventDefault();
      submenuOpen = false;
      menu?.querySelector<HTMLButtonElement>('button[data-submenu-trigger="true"]')?.focus();
      return;
    }

    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key) || !menu) return;
    const scope = submenuOpen
      ? menu.querySelector('[data-submenu-panel]') ?? menu
      : menu;
    const items = Array.from(scope.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)'));
    if (!items.length) return;
    event.preventDefault();
    const current = items.findIndex((item) => item === document.activeElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    const index =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? items.length - 1
          : (current + step + items.length) % items.length;
    items[index].focus();
  }

  function run(fn: () => void) {
    fn();
    contextMenu.close();
  }

  function openArtistTrack(name: string) {
    goArtistByName(name);
  }

  function resolveAlbumId(t: Track): string | null {
    if (t.albumId) return t.albumId;
    if (!t.album) return null;
    const match = libraryStore.albums.find(
      (a) =>
        a.title.toLowerCase() === t.album.toLowerCase() &&
        (!t.artist || a.artist.toLowerCase().includes(t.artist.toLowerCase()))
    );
    return match?.id ?? null;
  }

  function trackShareLink(t: Track): string | null {
    const vid = t.videoId?.trim() || (t.id && !t.id.startsWith('local-') && t.id.length === 11 ? t.id : '');
    if (vid) return `https://music.youtube.com/watch?v=${vid}`;
    if (t.localPath) return t.localPath;
    return null;
  }

  async function copyLink(t: Track) {
    const link = trackShareLink(t);
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      featuresStore.showToast({ title: 'Link copied', severity: 'success' });
    } catch {
      featuresStore.showToast({ title: 'Could not copy link', severity: 'warning' });
    }
  }

  async function copyTitleArtist(t: Track) {
    const text = `${t.title} — ${t.artist}`;
    try {
      await navigator.clipboard.writeText(text);
      featuresStore.showToast({
        title: 'Title and artist copied',
        severity: 'success',
      });
    } catch {
      featuresStore.showToast({ title: 'Could not copy to clipboard', severity: 'warning' });
    }
  }

  async function excludeTrack(t: Track) {
    if (isTauriRuntime()) {
      await ipcInvoke('library_exclusions_add', { name: t.title, artist: t.artist }).catch(() => null);
      void libraryStore.loadHomeFeed(true);
    }
    featuresStore.showToast({
      title: "Won't recommend",
      subtitle: `${t.title} — ${t.artist}`,
      severity: 'info',
    });
  }

  function startNewPlaylistForTrack(t: Track) {
    pendingTrackForPlaylist = t;
    contextMenu.close();
  }

  function closeNewPlaylist() {
    pendingTrackForPlaylist = null;
    void tick().then(() => previousFocus?.focus({ preventScroll: true }));
  }

  async function addToPlaylist(playlist: Playlist, track: Track): Promise<boolean> {
    const added = await libraryStore.addTrackToPlaylist(playlist.id, track);
    featuresStore.showToast({
      title: added ? `Added to ${playlist.name}` : `Could not add to ${playlist.name}`,
      subtitle: track.title,
      severity: added ? 'success' : 'warning',
    });
    return added;
  }

  let albumTarget = $derived(track ? resolveAlbumId(track) : null);
  let shareLink = $derived(track ? trackShareLink(track) : null);
  let artistList = $derived.by(() => {
    if (!track) return [];
    if (track.artists.length > 0) return track.artists.slice(0, 4);
    return track.artist ? [track.artist] : [];
  });
  let propDownload = $derived(
    contextMenu.propertiesTrack
      ? libraryStore.downloads.find((d) => d.trackId === contextMenu.propertiesTrack!.id)
      : undefined
  );
  let propOrigin = $derived.by(() => {
    const pt = contextMenu.propertiesTrack;
    if (!pt) return { kind: 'Stream', path: '' };
    if (pt.localPath) return { kind: 'Local file', path: pt.localPath };
    if (pt.isLocal) return { kind: 'Local file', path: '' };
    if (propDownload?.localPath) return { kind: 'Download', path: propDownload.localPath };
    return { kind: 'Stream', path: '' };
  });
</script>

<svelte:window
  onclick={() => contextMenu.open && contextMenu.close()}
  onkeydown={(e) => !e.defaultPrevented && e.key === 'Escape' && contextMenu.close()}
/>

{#if track}
  <div
    role="menu"
    aria-label="Track actions"
    bind:this={menu}
    tabindex="-1"
    data-lw="context-menu"
    class="lw-popover lw-has-cascade lw-flyout-surface fixed z-50 w-[264px]"
    data-open={contextMenu.open}
    inert={!contextMenu.open}
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    style:--lw-popover-origin="top left"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onMenuKeydown}
  >
    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => playTrackNow(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="play" size={15} />
      <span class="min-w-0 flex-1 truncate">Play</span>
      <span class="lw-menu-hint">Enter</span>
    </button>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => playTrackNext(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="plus" size={15} />
      <span class="min-w-0 flex-1 truncate">Play next</span>
    </button>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => enqueueTrack(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="list" size={15} />
      <span class="min-w-0 flex-1 truncate">Add to queue</span>
    </button>

    <div class="lw-menu-sep"></div>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => toggleTrackLike(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="heart" size={15} filled={liked} />
      <span class="min-w-0 flex-1 truncate">{liked ? 'Unlike' : 'Like'}</span>
    </button>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => downloadTrack(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="download" size={15} />
      <span class="min-w-0 flex-1 truncate">Download</span>
      <span class="lw-menu-hint">Ctrl+D</span>
    </button>

    <!-- Submenu cascade: Add to playlist (matches waveAddToPlaylistEntry in menus.dart) -->
    <div
      class="relative"
      role="none"
      onpointerenter={hoverSubmenuRow}
      onpointerleave={scheduleCloseSubmenu}
    >
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        aria-haspopup="menu"
        aria-expanded={submenuOpen}
        data-submenu-trigger="true"
        data-highlighted={submenuOpen}
        onclick={(e) => {
          e.stopPropagation();
          openSubmenuNow();
        }}
        class="lw-menu-row"
      >
        <WaveIcon name="playlists" size={15} />
        <span class="min-w-0 flex-1 truncate">Add to playlist</span>
        <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
      </button>

      {#if submenuOpen}
        <!-- 6px padded bridge prevents diagonal mouse gap from closing submenu -->
        <div
          role="menu"
          data-submenu-panel
          tabindex="-1"
          use:clampSubmenu
          onpointerenter={cancelSubmenuTimers}
          onpointerleave={scheduleCloseSubmenu}
          class="absolute z-20 w-[236px] {submenuOpenUp ? 'bottom-[-4px]' : 'top-[-4px]'} {submenuFlipLeft ? 'right-full pr-1.5' : 'left-full pl-1.5'}"
        >
          <div class="lw-flyout-surface max-h-[300px] overflow-y-auto">
            {#each customPlaylists as pl (pl.id)}
              <button
                type="button"
                role="menuitem"
                tabindex="-1"
                onclick={() => run(() => void addToPlaylist(pl, track))}
                class="lw-menu-row"
              >
                <WaveIcon name="playlists" size={15} />
                <span class="min-w-0 flex-1 truncate">{pl.name}</span>
              </button>
            {/each}

            {#if customPlaylists.length > 0}
              <div class="lw-menu-sep"></div>
            {/if}

            <button
              type="button"
              role="menuitem"
              tabindex="-1"
              onclick={() => startNewPlaylistForTrack(track)}
              class="lw-menu-row"
            >
              <WaveIcon name="plus" size={15} />
              <span class="min-w-0 flex-1 truncate">New playlist…</span>
            </button>
          </div>
        </div>
      {/if}
    </div>

    <div class="lw-menu-sep"></div>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      disabled={!albumTarget}
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => albumTarget && run(() => goAlbum(albumTarget))}
      class="lw-menu-row"
    >
      <WaveIcon name="album" size={15} />
      <span class="min-w-0 flex-1 truncate">Go to album</span>
    </button>

    {#each artistList as artistName (artistName)}
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        onpointerenter={scheduleCloseSubmenu}
        onclick={() => run(() => openArtistTrack(artistName))}
        class="lw-menu-row"
      >
        <WaveIcon name="artist" size={15} />
        <span class="min-w-0 flex-1 truncate">Go to {artistName}</span>
      </button>
    {/each}

    <div class="lw-menu-sep"></div>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      disabled={!shareLink}
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => void copyLink(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="link" size={15} />
      <span class="min-w-0 flex-1 truncate">Copy link</span>
    </button>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => void copyTitleArtist(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="copy" size={15} />
      <span class="min-w-0 flex-1 truncate">Copy title and artist</span>
    </button>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => contextMenu.openProperties(track)}
      class="lw-menu-row"
    >
      <WaveIcon name="info" size={15} />
      <span class="min-w-0 flex-1 truncate">Properties</span>
    </button>

    {#if contextMenu.removeLabel && contextMenu.onRemove}
      <div class="lw-menu-sep"></div>
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        onpointerenter={scheduleCloseSubmenu}
        onclick={() => run(() => contextMenu.onRemove?.())}
        class="lw-menu-row"
      >
        <WaveIcon name="trash" size={15} />
        <span class="min-w-0 flex-1 truncate">{contextMenu.removeLabel}</span>
        <span class="lw-menu-hint">Del</span>
      </button>
    {/if}

    <div class="lw-menu-sep"></div>

    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      onpointerenter={scheduleCloseSubmenu}
      onclick={() => run(() => void excludeTrack(track))}
      class="lw-menu-row"
    >
      <WaveIcon name="blocked" size={15} />
      <span class="min-w-0 flex-1 truncate">Don't recommend</span>
    </button>

    {#if uiSlots.trackContextMenuItems.length > 0}
      <div class="lw-menu-sep"></div>
      {#each uiSlots.trackContextMenuItems as item (item.id)}
        <button
          type="button"
          role="menuitem"
          tabindex="-1"
          onpointerenter={scheduleCloseSubmenu}
          onclick={() => run(() => item.onClick(track))}
          class="lw-menu-row"
        >
          <WaveIcon name="plugin" size={15} />
          <span class="min-w-0 flex-1 truncate">{item.label}</span>
        </button>
      {/each}
    {/if}
  </div>
{/if}

<!-- Track Properties dialog (matches showWaveTrackProperties in menus.dart) -->
<dialog
  bind:this={propsDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => contextMenu.closeProperties()}
  onclose={() => contextMenu.closeProperties()}
  aria-label="Properties"
  class="w-[min(440px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  {#if contextMenu.propertiesTrack}
    {@const pt = contextMenu.propertiesTrack}
    <h2 class="text-[19px] font-semibold tracking-[-0.3px]">Properties</h2>

    <div class="mt-4 space-y-3.5 select-text">
      <div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">Title</div>
        <div class="mt-0.5 text-[15px] font-semibold leading-snug">{pt.title}</div>
      </div>

      <div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">Artist</div>
        <div class="mt-0.5 text-[15px] font-semibold leading-snug">{pt.artist}</div>
      </div>

      {#if pt.album}
        <div>
          <div class="text-[12px] text-[var(--lw-text-secondary)]">Album</div>
          <div class="mt-0.5 text-[15px] font-semibold leading-snug">{pt.album}</div>
        </div>
      {/if}

      <div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">Duration</div>
        <div class="mt-0.5 text-[15px] font-semibold leading-snug lw-tabular">
          {pt.durationSec > 0 ? formatDuration(pt.durationSec) : 'Unknown'}
        </div>
      </div>

      <div>
        <div class="text-[12px] text-[var(--lw-text-secondary)]">Source</div>
        <div class="mt-0.5 text-[15px] font-semibold leading-snug">{propOrigin.kind}</div>
      </div>

      {#if pt.codec}
        <div>
          <div class="text-[12px] text-[var(--lw-text-secondary)]">Quality</div>
          <div class="mt-0.5 text-[15px] font-semibold leading-snug lw-tabular">
            {pt.codec}{pt.sampleRateHz > 0 ? ` · ${(pt.sampleRateHz / 1000).toFixed(pt.sampleRateHz % 1000 === 0 ? 0 : 1)} kHz` : ''}{pt.bitDepth > 0 ? ` / ${pt.bitDepth}-bit` : ''}{pt.bitrateKbps > 0 ? ` · ${pt.bitrateKbps} kbps` : ''}
          </div>
        </div>
      {/if}

      {#if propOrigin.path}
        <div>
          <div class="text-[12px] text-[var(--lw-text-secondary)]">Path</div>
          <div class="mt-0.5 break-all text-[14px] font-semibold leading-snug">{propOrigin.path}</div>
        </div>
      {/if}
    </div>

    <div class="mt-6 flex justify-end">
      <Buttons variant="primary" size="sm" onclick={() => propsDialog?.close()}>Close</Buttons>
    </div>
  {/if}
</dialog>

<PlaylistDialogs open={Boolean(pendingTrackForPlaylist)} track={pendingTrackForPlaylist} onClose={closeNewPlaylist} />
