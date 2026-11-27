<script lang="ts">
  import { dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import lastWaveIcon from '../../../src-tauri/icons/icon.png';

  import { getCurrentWindow } from '@tauri-apps/api/window';

  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  let searchQuery = $state('');
  let searchFocused = $state(false);
  let searchPointerFocus = $state(false);
  let menuOpen = $state(false);
  let accountName = $derived(settingsStore.ytAccountName || 'Guest');
  let showSearchSuggestions = $derived(searchFocused && Boolean(searchQuery.trim() || libraryStore.searchHistory.length));

  let liveTracks = $derived(
    searchQuery.trim()
      ? libraryStore.tracks
          .filter(
            (t) =>
              t.title.toLowerCase().includes(searchQuery.trim().toLowerCase()) ||
              t.artist.toLowerCase().includes(searchQuery.trim().toLowerCase())
          )
          .slice(0, 4)
      : []
  );

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    openSearch(searchQuery);
  }

  function openSearch(query: string) {
    const trimmed = query.trim();
    if (trimmed) libraryStore.pushSearchHistory(trimmed);
    searchFocused = false;
    router.navigate(trimmed ? `/search/${encodeURIComponent(trimmed)}` : '/search');
  }
</script>

<WaveHaze
  level="l1"
  enableBlur={false}
  dataLw="title-bar"
  class="relative z-30"
>
  <div class="title-bar" data-tauri-drag-region>
    <button type="button" class="title-brand" aria-label="LastWave home" onclick={() => router.navigate('/home')}>
      <img class="brand-mark" src={lastWaveIcon} alt="" width="33" height="33" draggable="false" />
      {#if windowState.railExpanded}<span class="lw-display brand-wordmark">LastWave</span>{/if}
    </button>
    <div class="flex items-center gap-0.5 pl-2">
      <button
        type="button"
        onclick={() => windowState.toggleRail()}
        title="Toggle navigation"
        aria-expanded={windowState.railExpanded}
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      >
        <WaveIcon name="layers" size={16} />
      </button>
      <button
        type="button"
        disabled={!router.canGoBack}
        onclick={() => router.back()}
        title="Back"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] disabled:cursor-default disabled:opacity-30"
      >
        <WaveIcon name="chevronLeft" size={16} />
      </button>
      <button
        type="button"
        disabled={!router.canGoForward}
        onclick={() => router.forward()}
        title="Forward"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] disabled:cursor-default disabled:opacity-30"
      >
        <WaveIcon name="chevronRight" size={16} />
      </button>
    </div>
    <div
      class="relative ml-2 w-[clamp(180px,31vw,370px)]"
      role="search"
      onfocusin={() => (searchFocused = true)}
      onfocusout={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) searchFocused = false;
      }}
    >
      <form onsubmit={submitSearch} class="relative flex items-center">
        <WaveIcon name="search" size={14} class="pointer-events-none absolute left-2.5 text-[var(--lw-text-tertiary)]" />
        <input
          type="text"
          bind:value={searchQuery}
          oninput={() => (searchFocused = true)}
          onpointerdown={() => (searchPointerFocus = true)}
          onblur={() => (searchPointerFocus = false)}
          onkeydown={(event) => { if (event.key === 'Escape') searchFocused = false; }}
          aria-label="Search music"
          placeholder="Search tracks, artists, albums"
          class="h-9 w-full rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] pr-7 pl-8 text-[13px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)]"
          class:search-pointer-focus={searchPointerFocus}
        />
        {#if searchQuery}
          <button
            type="button"
            onclick={() => (searchQuery = '')}
            aria-label="Clear search"
            class="absolute right-1.5 flex h-5 w-5 cursor-pointer items-center justify-center rounded text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="close" size={12} />
          </button>
        {/if}
      </form>
        <div
          data-open={showSearchSuggestions}
          inert={!showSearchSuggestions}
          class="lw-popover absolute top-11 left-0 z-50 w-full rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 shadow-[var(--lw-shadow-flyout)]"
          style:--lw-popover-origin="top left"
        >
          {#if !searchQuery.trim()}
            <div class="px-2 py-1 text-[10.5px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
              Recent
            </div>
            {#each libraryStore.searchHistory.slice(0, 4) as item (item)}
              <button
                type="button"
                onclick={() => {
                  searchQuery = item;
                  openSearch(item);
                }}
                class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
              >
                <WaveIcon name="clock" size={13} class="text-[var(--lw-text-tertiary)]" />
                <span class="truncate">{item}</span>
              </button>
            {/each}
          {:else if liveTracks.length > 0}
            {#each liveTracks as track (track.id)}
              <button
                type="button"
                onclick={() => { searchFocused = false; playTrackNow(track); }}
                class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
              >
                <Artwork src={track.artworkUrl} size={32} radius={6} />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[12.5px] font-semibold">{track.title}</div>
                  <div class="truncate text-[11px] text-[var(--lw-text-secondary)]">Song · {track.artist}</div>
                </div>
              </button>
            {/each}
          {:else}
            <div class="px-2.5 py-2 text-[12px] text-[var(--lw-text-tertiary)]">Press Enter to search all results</div>
          {/if}
        </div>
    </div>
    <button
      type="button"
      onclick={() => (windowState.commandPaletteOpen = true)}
      title="Commands (Ctrl+K)"
      class="command-trigger ml-1 hidden h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] sm:flex"
    >
      <WaveIcon name="terminal" size={15} />
    </button>
    <div class="h-full min-w-4 flex-1" data-tauri-drag-region></div>
    <div class="flex items-center gap-1">
      <button
        type="button"
        onclick={() => router.navigate('/settings/integrations')}
        title="Manage music accounts"
        class="flex h-8 max-w-[130px] cursor-pointer items-center gap-1.5 rounded-[9px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-2 text-[12px] font-semibold text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="artist" size={14} class="shrink-0 text-[var(--lw-accent)]" />
        <span class="truncate">{accountName}</span>
      </button>
      <div class="relative" use:dismissFlyout={{ open: menuOpen, close: () => (menuOpen = false) }}>
        <button
          type="button"
          onclick={() => (menuOpen = !menuOpen)}
          title="App menu"
          aria-expanded={menuOpen}
          aria-controls="lw-app-menu"
          class="lw-press flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="more" size={15} />
        </button>
          <div id="lw-app-menu" data-open={menuOpen} inert={!menuOpen} class="lw-popover absolute top-9 right-0 z-50 w-[190px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 text-[12.5px] shadow-[var(--lw-shadow-flyout)]">
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/settings'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="settings" size={14} /> Settings
            </button>
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/store'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="plugin" size={14} /> Plugins & Themes
            </button>
            <button type="button" onclick={() => { menuOpen = false; windowState.updateDialogOpen = true; }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="sparkles" size={14} /> Check for updates
            </button>
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/support'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="info" size={14} /> Keyboard shortcuts
            </button>
          </div>
      </div>
      {#if appWindow}
      <div class="native-controls">
        <button
          type="button"
          onclick={() => appWindow.minimize()}
          title="Minimize"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="h-[1.5px] w-2.5 bg-current"></span>
        </button>
        <button
          type="button"
          onclick={() => appWindow?.toggleMaximize()}
          title="Maximize"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="h-2.5 w-2.5 rounded-[2px] border border-current"></span>
        </button>
        <button
          type="button"
          onclick={() => appWindow?.close()}
          title="Close"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-danger)] hover:text-white"
        >
          <WaveIcon name="close" size={14} />
        </button>
      </div>
      {/if}
    </div>
  </div>
</WaveHaze>

<style>
  .title-bar {
    display: flex;
    height: var(--lw-titlebar-h);
    align-items: center;
    padding-right: 8px;
    border-bottom: 1px solid var(--lw-divider);
    background: var(--lw-rail-translucent);
  }

  .title-brand {
    display: flex;
    height: 100%;
    flex: 0 0 var(--lw-shell-rail-width);
    align-items: center;
    gap: 9px;
    padding-left: 15px;
    color: var(--lw-text-primary);
    cursor: pointer;
  }

  .title-brand:focus-visible { outline-offset: -4px; }
  .brand-mark {
    width: 33px;
    height: 33px;
    flex: none;
    transition: transform var(--lw-motion-fast) var(--ease-wave);
  }
  .title-brand:active .brand-mark { transform: scale(calc(1 - 0.04 * var(--lw-motion-factor))); }
  .brand-wordmark { font-size: 20px; font-weight: 700; }
  .search-pointer-focus:focus { outline: none; }
  .command-trigger:focus-visible {
    outline: none;
    background: var(--lw-wash-pressed);
    color: var(--lw-text-primary);
  }
  @media (forced-colors: active) {
    .command-trigger:focus-visible { outline: 2px solid Highlight; }
  }
  .native-controls { display: flex; align-items: center; margin-left: 5px; padding-left: 5px; border-left: 1px solid var(--lw-divider); }
</style>
