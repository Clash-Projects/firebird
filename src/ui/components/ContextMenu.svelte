<script lang="ts">
  import { tick } from 'svelte';
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import {
    downloadTrack,
    enqueueTrack,
    playTrackNext,
    playTrackNow,
    toggleTrackLike,
  } from '$src/app/trackActions';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { formatDuration } from '$src/state/player.svelte';
  import type { Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import Artwork from './Artwork.svelte';
  import { contextMenu } from './contextMenuState.svelte';

  let track = $derived(contextMenu.track);
  let menu = $state<HTMLDivElement>();
  let submenuOpen = $state(false);
  let submenuFlipLeft = $state(false);
  let previousFocus: HTMLElement | null = null;
  let liked = $derived(track ? libraryStore.isLiked(track.id) : false);
  let customPlaylists = $derived(
    libraryStore.playlists.filter((p) => p.id !== 'pl-liked' && p.source === 'local')
  );

  // Properties modal & New playlist modal state
  let propsDialog = $state<HTMLDialogElement | null>(null);
  let newPlDialog = $state<HTMLDialogElement | null>(null);
  let newPlName = $state('');
  let pendingTrackForPlaylist = $state<Track | null>(null);
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
      contextMenu.clampToViewport(rect.width, rect.height);
      const scaleRaw = getComputedStyle(document.documentElement).getPropertyValue('--lw-ui-scale');
      const scale = parseFloat(scaleRaw) > 0 ? parseFloat(scaleRaw) : 1;
      submenuFlipLeft = contextMenu.x + rect.width + 216 > window.innerWidth / scale;
      menu.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus({ preventScroll: true });
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

  function openSubmenu() {
    if (closeSubmenuTimer) {
      clearTimeout(closeSubmenuTimer);
      closeSubmenuTimer = null;
    }
    submenuOpen = true;
  }

  function scheduleCloseSubmenu() {
    if (closeSubmenuTimer) clearTimeout(closeSubmenuTimer);
    closeSubmenuTimer = setTimeout(() => {
      submenuOpen = false;
    }, 120);
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
      submenuOpen = true;
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
    const items = Array.from(scope.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]'));
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
    const clean = name.trim();
    const match =
      libraryStore.artists.find((a) => a.name.toLowerCase() === clean.toLowerCase()) ??
      libraryStore.artists.find((a) => a.name.toLowerCase().includes(clean.toLowerCase()));
    goArtist(match?.id ?? encodeURIComponent(clean));
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

  async function copyTitleArtist(t: Track) {
    const text = `${t.title} — ${t.artist}`;
    try {
      await navigator.clipboard.writeText(text);
      featuresStore.showToast({
        title: 'Copied to clipboard',
        subtitle: text,
        severity: 'success',
      });
    } catch {
      featuresStore.showToast({ title: 'Could not copy to clipboard', severity: 'warning' });
    }
  }

  function startNewPlaylistForTrack(t: Track) {
    pendingTrackForPlaylist = t;
    newPlName = '';
    contextMenu.close();
    newPlDialog?.showModal();
  }

  function submitNewPlaylist(e: SubmitEvent) {
    e.preventDefault();
    const clean = newPlName.trim();
    if (!clean || !pendingTrackForPlaylist) return;
    const pl = libraryStore.createPlaylist(clean);
    libraryStore.addTrackToPlaylist(pl.id, pendingTrackForPlaylist.id);
    featuresStore.showToast({
      title: `Added to ${pl.name}`,
      subtitle: pendingTrackForPlaylist.title,
      severity: 'success',
    });
    pendingTrackForPlaylist = null;
    newPlDialog?.close();
  }

  let albumTarget = $derived(track ? resolveAlbumId(track) : null);
  let propDownload = $derived(
    contextMenu.propertiesTrack
      ? libraryStore.downloads.find((d) => d.trackId === contextMenu.propertiesTrack!.id)
      : undefined
  );
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
    class="lw-popover fixed z-50"
    data-open={contextMenu.open}
    inert={!contextMenu.open}
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    style:--lw-popover-origin="top left"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onMenuKeydown}
  >
    <WaveHaze
      level="l3"
      dataLw="context-menu"
      class="relative w-[236px] rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1 shadow-[var(--lw-shadow-flyout)]"
    >
      <div class="flex items-center gap-2.5 border-b border-[var(--lw-divider)] px-2.5 py-2">
        <Artwork src={track.artworkUrl} label={track.title} artist={track.artist} kind="track" size={30} radius={6} />
        <div class="min-w-0 flex-1">
          <div class="truncate text-[12.5px] font-semibold leading-tight text-[var(--lw-text-primary)]">{track.title}</div>
          <div class="truncate text-[11px] leading-tight text-[var(--lw-text-secondary)]">{track.artist}</div>
        </div>
      </div>

      <div class="mt-1 space-y-0.5 text-[12.5px]">
        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => playTrackNow(track))} class="ctx-item">
          <WaveIcon name="play" size={14} />
          <span class="flex-1 truncate">Play</span>
          <span class="ctx-hint">Enter</span>
        </button>
        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => playTrackNext(track))} class="ctx-item">
          <WaveIcon name="next" size={14} />
          <span class="flex-1 truncate">Play next</span>
        </button>
        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => enqueueTrack(track))} class="ctx-item">
          <WaveIcon name="addToQueue" size={14} />
          <span class="flex-1 truncate">Add to queue</span>
        </button>

        <div class="my-1 h-px bg-[var(--lw-divider)]"></div>

        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => toggleTrackLike(track))} class="ctx-item">
          <WaveIcon name="heart" size={14} filled={liked} />
          <span class="flex-1 truncate">{liked ? 'Unlike' : 'Like'}</span>
        </button>
        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => downloadTrack(track))} class="ctx-item">
          <WaveIcon name="download" size={14} />
          <span class="flex-1 truncate">Download</span>
          <span class="ctx-hint">Ctrl+D</span>
        </button>

        <!-- Submenu cascade: Add to playlist -->
        <div
          class="relative"
          role="none"
          onmouseenter={openSubmenu}
          onmouseleave={scheduleCloseSubmenu}
        >
          <button
            type="button"
            role="menuitem"
            tabindex="-1"
            aria-haspopup="menu"
            aria-expanded={submenuOpen}
            data-submenu-trigger="true"
            onclick={(e) => {
              e.stopPropagation();
              submenuOpen = !submenuOpen;
            }}
            class="ctx-item"
          >
            <WaveIcon name="plus" size={14} />
            <span class="flex-1 truncate">Add to playlist</span>
            <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
          </button>

          {#if submenuOpen}
            <div
              role="menu"
              data-submenu-panel
              tabindex="-1"
              onmouseenter={openSubmenu}
              onmouseleave={scheduleCloseSubmenu}
              class="absolute top-[-4px] z-10 w-[208px] {submenuFlipLeft ? 'right-full mr-1' : 'left-full ml-1'}"
            >
              <WaveHaze
                level="l3"
                class="rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1 shadow-[var(--lw-shadow-flyout)]"
              >
                <div use:waveSmoothScroll class="lw-scroll max-h-[260px]">
                <button
                  type="button"
                  role="menuitem"
                  tabindex="-1"
                  onclick={() => startNewPlaylistForTrack(track)}
                  class="ctx-item font-medium text-[var(--lw-accent)]"
                >
                  <WaveIcon name="plus" size={13} />
                  <span class="truncate">New playlist...</span>
                </button>
                {#if customPlaylists.length > 0}
                  <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
                  {#each customPlaylists as pl (pl.id)}
                    <button
                      type="button"
                      role="menuitem"
                      tabindex="-1"
                      onclick={() =>
                        run(() => {
                          libraryStore.addTrackToPlaylist(pl.id, track.id);
                          featuresStore.showToast({
                            title: `Added to ${pl.name}`,
                            subtitle: track.title,
                            severity: 'success',
                          });
                        })}
                      class="ctx-item"
                    >
                      <WaveIcon name="queue" size={13} />
                      <span class="truncate">{pl.name}</span>
                    </button>
                  {/each}
                {/if}
                </div>
              </WaveHaze>
            </div>
          {/if}
        </div>

        <div class="my-1 h-px bg-[var(--lw-divider)]"></div>

        {#if albumTarget}
          <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => goAlbum(albumTarget))} class="ctx-item">
            <WaveIcon name="album" size={14} />
            <span class="flex-1 truncate">Go to album</span>
          </button>
        {/if}

        {#each track.artists.slice(0, 3) as artistName (artistName)}
          <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => openArtistTrack(artistName))} class="ctx-item">
            <WaveIcon name="artist" size={14} />
            <span class="flex-1 truncate">Go to {artistName}</span>
          </button>
        {/each}

        <div class="my-1 h-px bg-[var(--lw-divider)]"></div>

        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => void copyTitleArtist(track))} class="ctx-item">
          <WaveIcon name="copy" size={14} />
          <span class="flex-1 truncate">Copy title and artist</span>
        </button>

        <button type="button" role="menuitem" tabindex="-1" onclick={() => contextMenu.openProperties(track)} class="ctx-item">
          <WaveIcon name="info" size={14} />
          <span class="flex-1 truncate">Properties</span>
        </button>

        {#if contextMenu.removeLabel && contextMenu.onRemove}
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
          <button
            type="button"
            role="menuitem"
            tabindex="-1"
            onclick={() => run(() => contextMenu.onRemove?.())}
            class="ctx-item text-[var(--lw-danger)]"
          >
            <WaveIcon name="trash" size={14} />
            <span class="flex-1 truncate">{contextMenu.removeLabel}</span>
            <span class="ctx-hint">Del</span>
          </button>
        {/if}

        {#if uiSlots.trackContextMenuItems.length > 0}
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
          {#each uiSlots.trackContextMenuItems as item (item.id)}
            <button type="button" role="menuitem" tabindex="-1" onclick={() => run(() => item.onClick(track))} class="ctx-item">
              <WaveIcon name="plugin" size={14} />
              <span class="flex-1 truncate">{item.label}</span>
            </button>
          {/each}
        {/if}
      </div>
    </WaveHaze>
  </div>
{/if}

<!-- Track Properties dialog -->
<dialog
  bind:this={propsDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => contextMenu.closeProperties()}
  onclose={() => contextMenu.closeProperties()}
  aria-label="Track properties"
  class="w-[min(440px,calc(100vw-32px))] p-5 text-[var(--lw-text-primary)]"
>
  {#if contextMenu.propertiesTrack}
    {@const pt = contextMenu.propertiesTrack}
    <div class="flex items-center gap-3.5">
      <Artwork src={pt.artworkUrl} label={pt.title} artist={pt.artist} kind="track" size={68} radius={10} shadow="lift" />
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-[16px] font-semibold text-[var(--lw-text-primary)]">{pt.title}</h2>
        <p class="truncate text-[13px] text-[var(--lw-text-secondary)]">{pt.artist}</p>
        {#if pt.album}
          <p class="truncate text-[12px] text-[var(--lw-text-tertiary)]">{pt.album}</p>
        {/if}
      </div>
    </div>

    <div class="mt-4 divide-y divide-[var(--lw-divider)] border-y border-[var(--lw-divider)] text-[12.5px]">
      <div class="flex items-center justify-between py-2">
        <span class="text-[var(--lw-text-secondary)]">Duration</span>
        <span class="lw-tabular">{formatDuration(pt.durationSec)}</span>
      </div>
      <div class="flex items-center justify-between py-2">
        <span class="text-[var(--lw-text-secondary)]">Format</span>
        <span>{pt.codec} · {(pt.sampleRateHz / 1000).toFixed(1)} kHz / {pt.bitDepth}-bit</span>
      </div>
      <div class="flex items-center justify-between py-2">
        <span class="text-[var(--lw-text-secondary)]">Bitrate</span>
        <span class="lw-tabular">{pt.bitrateKbps} kbps</span>
      </div>
      <div class="flex items-center justify-between gap-4 py-2">
        <span class="shrink-0 text-[var(--lw-text-secondary)]">Source</span>
        <span class="truncate text-right text-[var(--lw-text-primary)]">
          {propDownload?.localPath ? `Downloaded (${propDownload.localPath})` : 'Streaming catalog'}
        </span>
      </div>
    </div>

    <div class="mt-4 flex justify-end">
      <button
        type="button"
        onclick={() => propsDialog?.close()}
        class="lw-press cursor-pointer rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface-overlay)] px-4 py-1.5 text-[12.5px] font-medium hover:bg-[var(--lw-wash-hover)]"
      >
        Close
      </button>
    </div>
  {/if}
</dialog>

<!-- New playlist quick-create dialog -->
<dialog
  bind:this={newPlDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => newPlDialog?.close()}
  aria-label="New playlist"
  class="w-[min(380px,calc(100vw-32px))] p-5 text-[var(--lw-text-primary)]"
>
  <form onsubmit={submitNewPlaylist}>
    <h2 class="text-[15px] font-semibold">New playlist</h2>
    <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
      Create a playlist and add {pendingTrackForPlaylist?.title ?? 'this track'}.
    </p>
    <div class="mt-3">
      <Input bind:value={newPlName} autofocus placeholder="Playlist name" />
    </div>
    <div class="mt-4 flex justify-end gap-2">
      <Buttons variant="ghost" size="sm" onclick={() => newPlDialog?.close()}>Cancel</Buttons>
      <Buttons type="submit" variant="primary" size="sm" disabled={!newPlName.trim()}>
        Create &amp; add
      </Buttons>
    </div>
  </form>
</dialog>

<style>
  .ctx-item {
    display: flex;
    width: 100%;
    height: 30px;
    align-items: center;
    gap: 10px;
    border-radius: 6px;
    padding: 0 9px;
    text-align: left;
    cursor: pointer;
    color: inherit;
    transition: background-color var(--lw-motion-fast) var(--ease-native);
  }

  .ctx-item:hover,
  .ctx-item:focus-visible {
    background: var(--lw-wash-hover);
    outline: none;
  }

  .ctx-hint {
    margin-left: auto;
    font-size: 11px;
    color: var(--lw-text-tertiary);
    font-variant-numeric: tabular-nums;
  }
</style>
