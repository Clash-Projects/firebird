<script lang="ts">
  import { dismissDialogOnBackdrop, dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { SearchResult, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { exactArtist, fetchSearchResults, openSearchResult, rememberSearchResults, searchMatches, searchResultDetails, type SearchResults } from '$src/ui/search/searchResults';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';
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

  function resultHit(result: SearchResult, query = ''): SearchHit {
    return {
      key: `${result.kind}:${result.item.id}`,
      kind: result.kind,
      ...searchResultDetails(result),
      track: result.kind === 'track' ? result.item : undefined,
      onSelect: () => {
        closeSearch();
        openSearchResult(result, query);
      },
    };
  }

  let searchHits = $derived.by<SearchHit[]>(() => {
    const query = searchQuery.trim();
    if (!query) {
      return libraryStore.searchHistory.slice(0, 4).map((item) => item.kind !== 'query' ? resultHit(item) : ({
        key: item.key,
        kind: 'recent',
        label: item.query,
        sub: 'Recent',
        onSelect: () => {
          searchQuery = item.query;
          openSearch(item.query);
        },
      }));
    }

    const limits = { track: 2, album: 1, artist: 1 };
    const hits: SearchHit[] = [];
    for (const match of matches) {
      if (limits[match.kind] === 0) continue;
      limits[match.kind] -= 1;
      hits.push(resultHit(match, query));
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
    if (artist) {
      closeSearch();
      openSearchResult({ kind: 'artist', item: artist }, query);
    } else openSearch(query);
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
        <WaveIcon name="panelLeft" size={15} />
      </button>
      <button
        type="button"
        disabled={!router.canGoBack}
        onclick={() => router.back()}
        use:waveTooltip={'Back'}
        aria-label="Back"
        class="bar-btn"
      >
        <WaveIcon name="chevronLeft" size={15} />
      </button>
      <button
        type="button"
        disabled={!router.canGoForward}
        onclick={() => router.forward()}
        use:waveTooltip={'Forward'}
        aria-label="Forward"
        class="bar-btn"
      >
        <WaveIcon name="chevronRight" size={15} />
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
        <WaveIcon name="search" size={15} class="pointer-events-none absolute left-2 text-[var(--lw-text-tertiary)]" />
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
          class="title-search-input"
        />
        {#if searchQuery}
          <button
            type="button"
            onclick={() => (searchQuery = '')}
            aria-label="Clear search"
            class="absolute right-1 flex h-5 w-5 cursor-pointer items-center justify-center rounded text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="close" size={10} />
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
                  <WaveIcon name="history" size={15} class="text-[var(--lw-text-secondary)]" />
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
      <WaveIcon name="command" size={15} />
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
        <BrandIcon name="telegram" size={15} />
      </button>
      <button
        type="button"
        onclick={() => openExternal(DISCORD_URL)}
        use:waveTooltip={'Discord server'}
        aria-label="Discord server"
        class="bar-btn"
      >
        <BrandIcon name="discord" size={15} />
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
              <WaveIcon name="contact" size={13} class="text-[var(--lw-text-secondary)]" />
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
            <WaveIcon name="edit" size={15} class="text-[var(--lw-text-secondary)]" />
            <span class="min-w-0 flex-1 truncate">{settingsStore.displayName.trim() ? `Name: ${settingsStore.displayName.trim()}` : 'Set your name…'}</span>
          </button>
          <button
            type="button"
            onclick={() => avatarFileInput?.click()}
            class="lw-menu-row"
          >
            <WaveIcon name="photo" size={15} class="text-[var(--lw-text-secondary)]" />
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
            <WaveIcon name="video" size={15} class="text-[var(--lw-text-secondary)]" />
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
            <WaveIcon name="history" size={15} />
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
            <WaveIcon name="chromeMinimize" size={12} />
          </button>
          <button
            type="button"
            onclick={() => appWindow.toggleMaximize()}
            use:waveTooltip={windowState.isMaximized ? 'Restore' : 'Maximize'}
            aria-label={windowState.isMaximized ? 'Restore' : 'Maximize'}
            class="win-btn"
          >
            <WaveIcon name={windowState.isMaximized ? 'chromeRestore' : 'chromeMaximize'} size={12} />
          </button>
          <button
            type="button"
            onclick={() => appWindow.close()}
            use:waveTooltip={'Close'}
            aria-label="Close"
            class="win-btn danger"
          >
            <WaveIcon name="close" size={12} />
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
    padding: 0 28px 1px 33px;
    border: 1px solid rgba(255, 255, 255, 0.071);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.059);
    box-shadow: inset 0 -1.25px 0 0 rgba(255, 255, 255, 0.545);
    color: var(--lw-text-primary);
    font-size: 13px;
    outline: none;
    transition:
      background-color var(--lw-motion-fast) var(--ease-wave),
      border-color var(--lw-motion-fast) var(--ease-wave),
      box-shadow var(--lw-motion-fast) var(--ease-wave);
  }

  .title-search-input:hover:not(:focus) {
    background: rgba(255, 255, 255, 0.082);
  }

  .title-search-input:focus {
    background: rgba(30, 30, 30, 0.702);
    box-shadow: inset 0 -2px 0 0 var(--lw-accent);
  }

  .title-search-input::placeholder {
    color: rgba(255, 255, 255, 0.78);
  }

  .title-search-input:focus::placeholder {
    color: rgba(255, 255, 255, 0.545);
  }

  :global(:root[data-theme="light"]) .title-search-input {
    border-color: rgba(0, 0, 0, 0.059);
    background: rgba(255, 255, 255, 0.702);
    box-shadow: inset 0 -1.25px 0 0 rgba(0, 0, 0, 0.447);
  }

  :global(:root[data-theme="light"]) .title-search-input:hover:not(:focus) {
    background: rgba(249, 249, 249, 0.502);
  }

  :global(:root[data-theme="light"]) .title-search-input:focus {
    background: #ffffff;
    box-shadow: inset 0 -2px 0 0 var(--lw-accent);
  }

  :global(:root[data-theme="light"]) .title-search-input::placeholder {
    color: rgba(0, 0, 0, 0.62);
  }

  :global(:root[data-theme="light"]) .title-search-input:focus::placeholder {
    color: rgba(0, 0, 0, 0.447);
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
