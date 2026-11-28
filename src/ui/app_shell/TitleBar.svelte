<script lang="ts">
  import { dismissDialogOnBackdrop, dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';
  import { exactArtist, fetchSearchResults, rememberSearchResults, searchMatches, type SearchResults } from '$src/ui/search/searchResults';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;
  const TELEGRAM_URL = 'https://t.me/clashprojects';
  const DISCORD_URL = 'https://discord.com/invite/TMCEPSUNk2';

  let searchQuery = $state('');
  let searchFocused = $state(false);
  let searchRoot = $state<HTMLDivElement>();
  let selectedKey = $state<string | null>(null);
  let menuOpen = $state(false);
  let profileMenuOpen = $state(false);
  let nameModalOpen = $state(false);
  let nameDraft = $state('');
  let nameDialogEl = $state<HTMLDialogElement | null>(null);
  let avatarFileInput: HTMLInputElement | undefined = $state();

  let remoteSearch = $state<{ query: string; results: SearchResults } | null>(null);
  let pendingSearch: { query: string; promise: Promise<SearchResults> } | null = null;
  let submitRevision = 0;

  function requestSearch(query: string): Promise<SearchResults> {
    if (remoteSearch?.query === query) return Promise.resolve(remoteSearch.results);
    if (pendingSearch?.query === query) return pendingSearch.promise;

    const promise = fetchSearchResults(query, { tracks: 6, albums: 4, artists: 5 }).then((results) => {
      if (pendingSearch?.promise === promise) {
        pendingSearch = null;
        if (searchQuery.trim() === query) {
          rememberSearchResults(results);
          remoteSearch = { query, results };
        }
      }
      return results;
    });
    pendingSearch = { query, promise };
    return promise;
  }

  $effect(() => {
    const query = searchQuery.trim();
    selectedKey = null;
    if (!query || !isTauriRuntime()) return;
    const timer = setTimeout(() => void requestSearch(query), 250);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    if (!nameDialogEl) return;
    if (nameModalOpen && !nameDialogEl.open) nameDialogEl.showModal();
    else if (!nameModalOpen && nameDialogEl.open) nameDialogEl.close();
  });

  let displayName = $derived(
    settingsStore.displayName.trim() ||
      settingsStore.ytAccountName.trim() ||
      settingsStore.ytAccountEmail.trim()
  );
  let avatarSrc = $derived(
    settingsStore.profilePictureUrl.trim() || settingsStore.ytAccountAvatar.trim()
  );
  let initialLetter = $derived(displayName ? displayName[0].toUpperCase() : '');
  let showSearchSuggestions = $derived(
    searchFocused && Boolean(searchQuery.trim() || libraryStore.searchHistory.length)
  );

  interface SearchHit {
    key: string;
    kind: 'recent' | 'track' | 'album' | 'artist' | 'seeAll';
    label: string;
    sub: string;
    art?: string;
    track?: Track;
    onSelect: () => void;
  }

  let matches = $derived(searchMatches(
    searchQuery,
    remoteSearch?.query === searchQuery.trim() ? remoteSearch.results : undefined,
  ));

  let searchHits = $derived.by<SearchHit[]>(() => {
    const query = searchQuery.trim();
    if (!query) {
      return libraryStore.searchHistory.slice(0, 4).map((item) => ({
        key: `recent:${item}`,
        kind: 'recent',
        label: item,
        sub: 'Recent',
        onSelect: () => {
          searchQuery = item;
          openSearch(item);
        },
      }));
    }

    const limits = { track: 2, album: 1, artist: 1 };
    const hits: SearchHit[] = [];
    for (const match of matches) {
      if (limits[match.kind] === 0) continue;
      limits[match.kind] -= 1;
      const key = `${match.kind}:${match.item.id}`;
      if (match.kind === 'track') {
        const track = match.item;
        hits.push({
          key,
          kind: 'track',
          label: track.title,
          sub: track.artist ? `Song · ${track.artist}` : 'Song',
          art: track.artworkUrl,
          track,
          onSelect: () => {
            closeSearch();
            libraryStore.pushSearchHistory(query);
            playTrackNow(track);
          },
        });
      } else if (match.kind === 'artist') {
        const artist = match.item;
        hits.push({
          key,
          kind: 'artist',
          label: artist.name,
          sub: 'Artist',
          art: artist.artworkUrl,
          onSelect: () => openArtistResult(artist.id, query),
        });
      } else {
        const album = match.item;
        hits.push({
          key,
          kind: 'album',
          label: album.title,
          sub: album.artist ? `Album · ${album.artist}` : 'Album',
          art: album.artworkUrl,
          onSelect: () => {
            closeSearch();
            libraryStore.pushSearchHistory(query);
            goAlbum(album.id);
          },
        });
      }
    }
    hits.push({
      key: 'all',
      kind: 'seeAll',
      label: `See all results for “${query}”`,
      sub: 'Open Search page',
      onSelect: () => openSearch(query),
    });
    return hits;
  });

  function closeSearch() {
    submitRevision += 1;
    searchFocused = false;
    selectedKey = null;
  }

  function isInsideSearch(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return Boolean(searchRoot?.contains(target) || (
      contextMenu.open && target.closest('[data-lw="context-menu"]')
    ));
  }

  function dismissSearchOutside(event: Event) {
    if (searchFocused && !isInsideSearch(event.target)) closeSearch();
  }

  function openSearchTrackMenu(event: MouseEvent, track: Track) {
    submitRevision += 1;
    contextMenu.showForTrack(event, track);
  }

  function openArtistResult(id: string, query: string) {
    closeSearch();
    libraryStore.pushSearchHistory(query);
    goArtist(id);
  }

  async function submitSearch(event: SubmitEvent) {
    event.preventDefault();
    const selected = searchFocused && searchHits.find((hit) => hit.key === selectedKey);
    if (selected) {
      selected.onSelect();
      return;
    }

    const query = searchQuery.trim();
    if (query && isTauriRuntime() && remoteSearch?.query !== query) {
      const revision = ++submitRevision;
      const path = router.path;
      await requestSearch(query);
      if (revision !== submitRevision || !searchFocused || query !== searchQuery.trim() || path !== router.path) return;
    }

    const artist = exactArtist(matches);
    if (artist) openArtistResult(artist.id, query);
    else openSearch(query);
  }

  function onSearchKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeSearch();
      (event.currentTarget as HTMLInputElement).blur();
      return;
    }
    if (!['ArrowDown', 'ArrowUp'].includes(event.key) || searchHits.length === 0) return;
    event.preventDefault();
    searchFocused = true;
    const index = searchHits.findIndex((hit) => hit.key === selectedKey);
    const next = event.key === 'ArrowDown'
      ? Math.min(searchHits.length - 1, index + 1)
      : Math.max(-1, index - 1);
    selectedKey = searchHits[next]?.key ?? null;
  }

  function openSearch(query: string) {
    const trimmed = query.trim();
    closeSearch();
    if (trimmed) libraryStore.pushSearchHistory(trimmed);
    router.navigate(trimmed ? `/search/${encodeURIComponent(trimmed)}` : '/search');
  }
  function openExternal(url: string) {
    window.open(url, '_blank', 'noopener,noreferrer');
  }

  function openNameEditor() {
    profileMenuOpen = false;
    nameDraft = settingsStore.displayName;
    nameModalOpen = true;
  }

  function saveDisplayName(value: string) {
    const clean = value.trim();
    settingsStore.displayName = clean;
    settingsStore.saveStringPref('lw_display_name', clean);
    nameModalOpen = false;
  }

  function removeProfilePicture() {
    profileMenuOpen = false;
    settingsStore.profilePicturePath = '';
    settingsStore.profilePictureUrl = '';
    settingsStore.saveStringPref('lw_profile_picture_path', '');
  }

  async function onPickAvatarFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    profileMenuOpen = false;
    if (!file) return;

    const dataUrl = await new Promise<string>((resolve) => {
      const img = new Image();
      const objUrl = URL.createObjectURL(file);
      img.onload = () => {
        const size = 256;
        const canvas = document.createElement('canvas');
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext('2d');
        if (ctx) {
          const minDim = Math.min(img.width, img.height);
          const sx = (img.width - minDim) / 2;
          const sy = (img.height - minDim) / 2;
          ctx.drawImage(img, sx, sy, minDim, minDim, 0, 0, size, size);
          resolve(canvas.toDataURL('image/jpeg', 0.88));
        } else {
          resolve('');
        }
        URL.revokeObjectURL(objUrl);
      };
      img.onerror = () => {
        URL.revokeObjectURL(objUrl);
        resolve('');
      };
      img.src = objUrl;
    });

    if (dataUrl) {
      settingsStore.profilePicturePath = dataUrl;
      settingsStore.profilePictureUrl = dataUrl;
      settingsStore.saveStringPref('lw_profile_picture_path', dataUrl);
    }
  }
</script>

<svelte:window onpointerdown={dismissSearchOutside} onfocusin={dismissSearchOutside} />

<WaveHaze
  level="l1"
  enableBlur={false}
  dataLw="title-bar"
  class="relative z-30"
>
  <div class="title-bar" data-tauri-drag-region>
    <div class="flex items-center gap-0.5 pl-1.5">
      <button
        type="button"
        onclick={() => windowState.toggleRail()}
        use:waveTooltip={'Toggle navigation'}
        aria-label="Toggle navigation"
        aria-expanded={windowState.railExpanded}
        class="bar-btn"
      >
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M2.5 4H13.5M2.5 8H13.5M2.5 12H13.5" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
        </svg>
      </button>
      <button
        type="button"
        disabled={!router.canGoBack}
        onclick={() => router.back()}
        use:waveTooltip={'Back'}
        aria-label="Back"
        class="bar-btn"
      >
        <WaveIcon name="chevronLeft" size={13} />
      </button>
      <button
        type="button"
        disabled={!router.canGoForward}
        onclick={() => router.forward()}
        use:waveTooltip={'Forward'}
        aria-label="Forward"
        class="bar-btn"
      >
        <WaveIcon name="chevronRight" size={13} />
      </button>
    </div>

    <div
      bind:this={searchRoot}
      class="relative ml-2 w-[clamp(140px,28vw,300px)]"
      role="search"
      onfocusin={() => (searchFocused = true)}
      onfocusout={(event) => {
        if (!isInsideSearch(event.relatedTarget)) {
          closeSearch();
        }
      }}
    >
      <form onsubmit={submitSearch} class="relative flex items-center">
        <WaveIcon name="search" size={14} class="pointer-events-none absolute left-2.5 text-[var(--lw-text-tertiary)]" />
        <input
          type="text"
          data-lw="title-search"
          bind:value={searchQuery}
          oninput={() => {
            submitRevision += 1;
            searchFocused = true;
          }}
          onkeydown={onSearchKeyDown}
          aria-label="Search songs, albums and artists"
          placeholder="Search"
          class="title-search-input lw-text-field"
        />
        {#if searchQuery}
          <button
            type="button"
            onclick={() => (searchQuery = '')}
            aria-label="Clear search"
            class="absolute right-1.5 flex h-5 w-5 cursor-pointer items-center justify-center rounded text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="close" size={11} />
          </button>
        {:else if !searchFocused}
          <span
            aria-hidden="true"
            class="pointer-events-none absolute right-2 text-[10px] font-medium tracking-[0.2px] text-[var(--lw-text-tertiary)]/70"
          >
            Ctrl F
          </span>
        {/if}
      </form>

      <div
        data-open={showSearchSuggestions}
        inert={!showSearchSuggestions}
        class="lw-popover lw-flyout-surface absolute top-9 left-0 z-50 w-full min-w-[320px]"
        style:--lw-popover-origin="top left"
      >
        {#each searchHits as hit (hit.key)}
          {@const track = hit.track}
          <div class="relative">
            <button
              type="button"
              data-search-result={hit.kind}
              onclick={hit.onSelect}
              oncontextmenu={(e) => track && openSearchTrackMenu(e, track)}
              class="flex w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-2.5 py-1.5 text-left transition-colors {track ? 'pr-10' : ''} {selectedKey === hit.key
                ? 'bg-[var(--lw-wash-pressed)]'
                : 'hover:bg-[var(--lw-wash-hover)]'}"
            >
              <span class="flex h-7 w-7 shrink-0 items-center justify-center">
                {#if hit.kind === 'recent'}
                  <WaveIcon name="clock" size={15} class="text-[var(--lw-text-secondary)]" />
                {:else if hit.kind === 'seeAll'}
                  <WaveIcon name="search" size={15} class="text-[var(--lw-text-secondary)]" />
                {:else}
                  <Artwork src={hit.art} label={hit.label} size={28} radius={8} circle={hit.kind === 'artist'} />
                {/if}
              </span>
              <div class="min-w-0 flex-1">
                <div class="truncate text-[13px] font-semibold text-[var(--lw-text-primary)]">{hit.label}</div>
                <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{hit.sub}</div>
              </div>
            </button>
            {#if track}
              <button
                type="button"
                aria-label={`Options for ${track.title}`}
                aria-haspopup="menu"
                use:waveTooltip={'Track options'}
                onclick={(e) => openSearchTrackMenu(e, track)}
                oncontextmenu={(e) => openSearchTrackMenu(e, track)}
                class="bar-btn absolute right-1 top-1/2 -translate-y-1/2 text-[var(--lw-text-secondary)]"
              >
                <WaveIcon name="more" size={14} />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    </div>

    <button
      type="button"
      onclick={() => (windowState.commandPaletteOpen = true)}
      use:waveTooltip={'Commands (Ctrl+K)'}
      aria-label="Commands (Ctrl+K)"
      class="bar-btn command-trigger ml-1 hidden sm:flex"
    >
      <WaveIcon name="terminal" size={15} />
    </button>

    <div
      class="h-full min-w-4 flex-1"
      role="presentation"
      data-tauri-drag-region
      ondblclick={() => void appWindow?.toggleMaximize()}
    ></div>

    <div class="flex items-center gap-1">
      <button
        type="button"
        onclick={() => openExternal(TELEGRAM_URL)}
        use:waveTooltip={'Telegram community'}
        aria-label="Telegram community"
        class="bar-btn"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm4.64 6.8c-.15 1.58-.8 5.42-1.13 7.19-.14.75-.42 1-.68 1.03-.58.05-1.02-.38-1.58-.75-.88-.58-1.38-.94-2.23-1.5-.99-.65-.35-1.01.22-1.59.15-.15 2.71-2.48 2.76-2.69a.2.2 0 0 0-.05-.18c-.06-.05-.14-.03-.21-.02-.09.02-1.49.95-4.22 2.79-.4.27-.76.41-1.08.4-.36-.01-1.04-.2-1.55-.37-.63-.2-1.12-.31-1.08-.66.02-.18.27-.36.74-.55 2.92-1.27 4.86-2.11 5.83-2.51 2.78-1.16 3.35-1.36 3.73-1.36.08 0 .27.02.39.12.1.08.13.19.14.27-.01.06.01.24 0 .38z" />
        </svg>
      </button>
      <button
        type="button"
        onclick={() => openExternal(DISCORD_URL)}
        use:waveTooltip={'Discord server'}
        aria-label="Discord server"
        class="bar-btn"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M19.27 5.33C17.94 4.71 16.5 4.26 15 4a.09.09 0 0 0-.07.03c-.18.33-.39.76-.53 1.09a16.09 16.09 0 0 0-4.8 0c-.14-.34-.35-.76-.54-1.09-.01-.02-.04-.03-.07-.03-1.5.26-2.93.71-4.27 1.33-.01 0-.02.01-.03.02-2.72 4.07-3.47 8.03-3.1 11.95 0 .02.01.04.03.05 1.8 1.32 3.53 2.12 5.24 2.65.03.01.06 0 .07-.02.4-.55.76-1.13 1.07-1.74.02-.04 0-.08-.04-.09-.57-.22-1.11-.48-1.64-.78-.04-.02-.04-.08-.01-.11.11-.08.22-.17.33-.25.02-.02.05-.02.07-.01 3.44 1.57 7.15 1.57 10.55 0 .02-.01.05-.01.07.01.11.09.22.17.33.26.04.03.04.09-.01.11-.52.31-1.07.56-1.64.78-.04.01-.05.06-.04.09.32.61.68 1.19 1.07 1.74.03.01.06.02.09.01 1.72-.53 3.45-1.33 5.25-2.65.02-.01.03-.03.03-.05.44-4.53-.73-8.46-3.1-11.95-.01-.01-.02-.02-.04-.02zM8.52 14.91c-1.03 0-1.89-.95-1.89-2.12s.84-2.12 1.89-2.12c1.06 0 1.9.96 1.89 2.12 0 1.17-.84 2.12-1.89 2.12zm6.97 0c-1.03 0-1.89-.95-1.89-2.12s.84-2.12 1.89-2.12c1.06 0 1.9.96 1.89 2.12 0 1.17-.83 2.12-1.89 2.12z" />
        </svg>
      </button>

      <input
        bind:this={avatarFileInput}
        type="file"
        accept="image/*"
        class="hidden"
        onchange={(e) => void onPickAvatarFile(e)}
      />

      <div class="relative mx-0.5" use:dismissFlyout={{ open: profileMenuOpen, close: () => (profileMenuOpen = false) }}>
        <button
          type="button"
          onclick={() => (profileMenuOpen = !profileMenuOpen)}
          use:waveTooltip={displayName || (settingsStore.ytConnected ? 'YouTube Music connected' : 'YouTube & profile')}
          aria-label={displayName ? `Profile, ${displayName}` : 'YouTube & profile'}
          aria-expanded={profileMenuOpen}
          class="lw-press flex h-[32px] w-[32px] cursor-pointer items-center justify-center rounded-full p-[2px] hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="flex h-[26px] w-[26px] items-center justify-center overflow-hidden rounded-full border border-[var(--lw-divider)] bg-[var(--lw-surface-raised)] text-[11px] font-semibold text-[var(--lw-text-primary)]">
            {#if avatarSrc}
              <img src={avatarSrc} alt="" class="h-full w-full object-cover" />
            {:else if initialLetter}
              {initialLetter}
            {:else}
              <WaveIcon name="artist" size={13} class="text-[var(--lw-text-secondary)]" />
            {/if}
          </span>
        </button>

        <div
          data-open={profileMenuOpen}
          inert={!profileMenuOpen}
          class="lw-popover lw-flyout-surface absolute top-9 right-0 z-50 w-[248px]"
        >
          <button
            type="button"
            onclick={openNameEditor}
            class="lw-menu-row"
          >
            <WaveIcon name="wand" size={15} class="text-[var(--lw-text-secondary)]" />
            <span class="min-w-0 flex-1 truncate">{settingsStore.displayName.trim() ? `Name: ${settingsStore.displayName.trim()}` : 'Set your name…'}</span>
          </button>
          <button
            type="button"
            onclick={() => avatarFileInput?.click()}
            class="lw-menu-row"
          >
            <WaveIcon name="palette" size={15} class="text-[var(--lw-text-secondary)]" />
            <span class="min-w-0 flex-1 truncate">{settingsStore.profilePictureUrl ? 'Change local picture…' : 'Choose local picture…'}</span>
          </button>
          {#if settingsStore.profilePictureUrl}
            <button
              type="button"
              onclick={removeProfilePicture}
              class="lw-menu-row"
            >
              <WaveIcon name="trash" size={15} class="text-[var(--lw-text-secondary)]" />
              <span class="min-w-0 flex-1 truncate">Remove local picture</span>
            </button>
          {/if}
          <div class="lw-menu-sep"></div>
          <button
            type="button"
            onclick={() => {
              profileMenuOpen = false;
              router.navigate('/settings/integrations');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="globe" size={15} class="text-[var(--lw-text-secondary)]" />
            <span class="min-w-0 flex-1 truncate">
              {settingsStore.ytConnected
                ? settingsStore.ytAccountName
                  ? `YouTube: ${settingsStore.ytAccountName}`
                  : 'YouTube Music connected'
                : 'Connect YouTube Music'}
            </span>
          </button>
        </div>
      </div>

      <div class="relative" use:dismissFlyout={{ open: menuOpen, close: () => (menuOpen = false) }}>
        <button
          type="button"
          onclick={() => (menuOpen = !menuOpen)}
          use:waveTooltip={'Menu'}
          aria-label="Menu"
          aria-expanded={menuOpen}
          aria-controls="lw-app-menu"
          class="bar-btn"
        >
          <WaveIcon name="more" size={15} />
        </button>
        <div
          id="lw-app-menu"
          data-open={menuOpen}
          inert={!menuOpen}
          class="lw-popover lw-flyout-surface absolute top-9 right-0 z-50 w-[216px]"
        >
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/search');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="search" size={15} />
            <span class="min-w-0 flex-1 truncate">Search</span>
            <span class="lw-menu-hint">Ctrl+K</span>
          </button>
          <div class="lw-menu-sep"></div>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/history');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="clock" size={15} />
            <span class="min-w-0 flex-1 truncate">History</span>
          </button>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/store');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="plugin" size={15} />
            <span class="min-w-0 flex-1 truncate">Plugins &amp; Themes</span>
          </button>
          <div class="lw-menu-sep"></div>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/settings');
            }}
            class="lw-menu-row"
          >
            <WaveIcon name="settings" size={15} />
            <span class="min-w-0 flex-1 truncate">Settings</span>
          </button>
        </div>
      </div>

      {#if appWindow}
        <div class="flex h-full items-center pl-0.5">
          <button
            type="button"
            onclick={() => appWindow.minimize()}
            use:waveTooltip={'Minimize'}
            aria-label="Minimize"
            class="win-btn"
          >
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
              <path d="M1.5 6H10.5" stroke="currentColor" stroke-width="1.15" />
            </svg>
          </button>
          <button
            type="button"
            onclick={() => appWindow.toggleMaximize()}
            use:waveTooltip={windowState.isMaximized ? 'Restore' : 'Maximize'}
            aria-label={windowState.isMaximized ? 'Restore' : 'Maximize'}
            class="win-btn"
          >
            {#if windowState.isMaximized}
              <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                <rect x="1.5" y="3.5" width="7" height="7" rx="0.8" stroke="currentColor" stroke-width="1.1" />
                <path d="M3.5 3.5V2C3.5 1.7 3.7 1.5 4 1.5H10C10.3 1.5 10.5 1.7 10.5 2V8C10.5 8.3 10.3 8.5 10 8.5H8.5" stroke="currentColor" stroke-width="1.1" />
              </svg>
            {:else}
              <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                <rect x="1.5" y="1.5" width="9" height="9" rx="1" stroke="currentColor" stroke-width="1.1" />
              </svg>
            {/if}
          </button>
          <button
            type="button"
            onclick={() => appWindow.close()}
            use:waveTooltip={'Close'}
            aria-label="Close"
            class="win-btn danger"
          >
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
              <path d="M2 2L10 10M10 2L2 10" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" />
            </svg>
          </button>
        </div>
      {/if}
    </div>
  </div>
</WaveHaze>

<dialog
  bind:this={nameDialogEl}
  use:dismissDialogOnBackdrop={() => (nameModalOpen = false)}
  onclose={() => (nameModalOpen = false)}
  closedby="any"
  aria-labelledby="lw-name-dialog-title"
  class="w-full max-w-[380px]"
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      saveDisplayName(nameDraft);
    }}
    class="space-y-4 p-5"
  >
    <h1 id="lw-name-dialog-title" class="text-[16px] font-bold">Your name</h1>
    <Input
      bind:value={nameDraft}
      autofocus
      placeholder="e.g. Ajisth (leave empty to use YouTube)"
    />
    <div class="flex items-center justify-end gap-2">
      {#if settingsStore.displayName}
        <Buttons variant="ghost" size="sm" onclick={() => saveDisplayName('')}>Clear</Buttons>
      {/if}
      <Buttons variant="secondary" size="sm" onclick={() => (nameModalOpen = false)}>Cancel</Buttons>
      <Buttons variant="primary" size="sm" type="submit">Save</Buttons>
    </div>
  </form>
</dialog>

<style>
  .title-bar {
    display: flex;
    height: var(--lw-titlebar-h, 44px);
    align-items: center;
    border-bottom: 1px solid var(--lw-divider);
    background: var(--lw-rail-translucent);
  }

  .bar-btn {
    display: flex;
    width: 36px;
    height: 36px;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 10px;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition:
      background-color var(--lw-motion-fast) ease,
      color var(--lw-motion-fast) ease,
      transform var(--lw-motion-fast) var(--ease-wave);
  }

  .bar-btn:disabled {
    cursor: default;
    opacity: 0.35;
  }

  .bar-btn:not(:disabled):hover {
    background: var(--lw-wash-hover);
    color: var(--lw-text-primary);
  }

  .bar-btn:not(:disabled):active {
    background: var(--lw-wash-pressed);
    transform: scale(0.93);
  }

  .title-search-input {
    width: 100%;
    height: 30px;
    padding: 0 28px 0 30px;
    border: 1px solid var(--lw-outline-soft);
    border-radius: 10px;
    background: var(--lw-surface);
    color: var(--lw-text-primary);
    font-size: 13px;
  }

  .title-search-input::placeholder {
    color: var(--lw-text-tertiary);
  }

  .win-btn {
    display: flex;
    width: 44px;
    height: var(--lw-titlebar-h, 44px);
    align-items: center;
    justify-content: center;
    color: var(--lw-text-secondary);
    cursor: pointer;
    transition: background-color 110ms ease, color 110ms ease;
  }

  .win-btn:hover {
    background: var(--lw-wash-hover);
    color: var(--lw-text-primary);
  }

  .win-btn.danger:hover {
    background: var(--lw-danger);
    color: #fff;
  }
</style>
