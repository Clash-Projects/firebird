<script lang="ts">
  import { dismissDialogOnBackdrop, dismissFlyout } from '$src/app/accessibility';
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtAlbum, mapYtArtist, mapYtTrack } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import type { Album, Artist, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import { contextMenu } from '$src/ui/components/contextMenuState.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { goAlbum, goArtist } from '$src/ui/navigation/waveNav';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;
  const TELEGRAM_URL = 'https://t.me/clashprojects';
  const DISCORD_URL = 'https://discord.com/invite/TMCEPSUNk2';

  let searchQuery = $state('');
  let liveQuery = $state('');
  let searchFocused = $state(false);
  let selectedIdx = $state(-1);
  let menuOpen = $state(false);
  let profileMenuOpen = $state(false);
  let nameModalOpen = $state(false);
  let nameDraft = $state('');
  let nameDialogEl = $state<HTMLDialogElement | null>(null);
  let avatarFileInput: HTMLInputElement | undefined = $state();

  let remoteSongs = $state<Track[]>([]);
  let remoteAlbums = $state<Album[]>([]);
  let remoteArtists = $state<Artist[]>([]);
  let searchSeq = 0;

  $effect(() => {
    const q = searchQuery.trim();
    selectedIdx = -1;
    if (!q) {
      liveQuery = '';
      remoteSongs = [];
      remoteAlbums = [];
      remoteArtists = [];
      return;
    }
    const seq = ++searchSeq;
    const timer = setTimeout(async () => {
      liveQuery = q;
      if (!isTauriRuntime()) return;
      try {
        const [songsRes, albumsRes, artistsRes] = await Promise.all([
          ipcInvoke<any[]>('yt_search_songs', { query: q, limit: 2 }).catch(() => []),
          ipcInvoke<any[]>('yt_search_albums', { query: q, limit: 1 }).catch(() => []),
          ipcInvoke<any[]>('yt_search_artists', { query: q, limit: 1 }).catch(() => []),
        ]);
        if (seq !== searchSeq) return;
        const songs = Array.isArray(songsRes) ? songsRes.map(mapYtTrack) : [];
        const albums = Array.isArray(albumsRes) ? albumsRes.map(mapYtAlbum) : [];
        const artists = Array.isArray(artistsRes) ? artistsRes.map(mapYtArtist) : [];
        remoteSongs = songs;
        remoteAlbums = albums;
        remoteArtists = artists;
        libraryStore.upsertTracks(songs);
        libraryStore.upsertAlbums(albums);
        libraryStore.upsertArtists(artists);
      } catch {
        // Fallback to local matches cleanly
      }
    }, 250);
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
    kind: 'recent' | 'track' | 'album' | 'artist' | 'seeAll';
    label: string;
    sub: string;
    art?: string;
    circle?: boolean;
    track?: Track;
    onSelect: () => void;
  }

  let searchHits = $derived.by<SearchHit[]>(() => {
    const q = searchQuery.trim();
    if (!q) {
      return libraryStore.searchHistory.slice(0, 4).map((item) => ({
        kind: 'recent',
        label: item,
        sub: 'Recent',
        onSelect: () => {
          searchQuery = item;
          openSearch(item);
        },
      }));
    }

    const ql = q.toLowerCase();
    const songs =
      liveQuery === q && remoteSongs.length > 0
        ? remoteSongs
        : libraryStore.tracks
            .filter(
              (t) =>
                t.title.toLowerCase().includes(ql) ||
                t.artist.toLowerCase().includes(ql)
            )
            .slice(0, 2);

    const albums =
      liveQuery === q && remoteAlbums.length > 0
        ? remoteAlbums
        : libraryStore.albums
            .filter(
              (a) =>
                a.title.toLowerCase().includes(ql) ||
                a.artist.toLowerCase().includes(ql)
            )
            .slice(0, 1);

    const artists =
      liveQuery === q && remoteArtists.length > 0
        ? remoteArtists
        : libraryStore.artists
            .filter((a) => a.name.toLowerCase().includes(ql))
            .slice(0, 1);

    const out: SearchHit[] = [];
    for (const s of songs.slice(0, 2)) {
      out.push({
        kind: 'track',
        label: s.title,
        sub: s.artist ? `Song · ${s.artist}` : 'Song',
        art: s.artworkUrl,
        track: s,
        onSelect: () => {
          searchFocused = false;
          playTrackNow(s);
        },
      });
    }
    for (const a of albums.slice(0, 1)) {
      out.push({
        kind: 'album',
        label: a.title,
        sub: a.artist ? `Album · ${a.artist}` : 'Album',
        art: a.artworkUrl,
        onSelect: () => {
          searchFocused = false;
          goAlbum(a.id);
        },
      });
    }
    for (const ar of artists.slice(0, 1)) {
      out.push({
        kind: 'artist',
        label: ar.name,
        sub: 'Artist',
        art: ar.artworkUrl,
        circle: true,
        onSelect: () => {
          searchFocused = false;
          goArtist(ar.id);
        },
      });
    }
    out.push({
      kind: 'seeAll',
      label: `See all results for “${q}”`,
      sub: 'Open Search page',
      onSelect: () => openSearch(q),
    });
    return out;
  });

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    if (selectedIdx >= 0 && selectedIdx < searchHits.length) {
      searchHits[selectedIdx].onSelect();
      return;
    }
    openSearch(searchQuery);
  }

  function onSearchKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      searchFocused = false;
      selectedIdx = -1;
      (event.currentTarget as HTMLInputElement).blur();
      return;
    }
    if (event.key === 'ArrowDown' && searchHits.length > 0) {
      event.preventDefault();
      searchFocused = true;
      selectedIdx = Math.min(searchHits.length - 1, selectedIdx + 1);
    } else if (event.key === 'ArrowUp' && searchHits.length > 0) {
      event.preventDefault();
      selectedIdx = Math.max(-1, selectedIdx - 1);
    }
  }

  function openSearch(query: string) {
    const trimmed = query.trim();
    if (trimmed) libraryStore.pushSearchHistory(trimmed);
    searchFocused = false;
    selectedIdx = -1;
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
      class="relative ml-2 w-[clamp(140px,28vw,300px)]"
      role="search"
      onfocusin={() => (searchFocused = true)}
      onfocusout={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          searchFocused = false;
        }
      }}
    >
      <form onsubmit={submitSearch} class="relative flex items-center">
        <WaveIcon name="search" size={14} class="pointer-events-none absolute left-2.5 text-[var(--lw-text-tertiary)]" />
        <input
          type="text"
          data-lw="title-search"
          bind:value={searchQuery}
          oninput={() => (searchFocused = true)}
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
        {/if}
      </form>

      <div
        data-open={showSearchSuggestions}
        inert={!showSearchSuggestions}
        class="lw-popover absolute top-9 left-0 z-50 w-full min-w-[320px] rounded-[10px] border border-[var(--lw-divider)] bg-[var(--lw-surface-raised)] p-1 shadow-[var(--lw-shadow-flyout)]"
        style:--lw-popover-origin="top left"
      >
        {#if !searchQuery.trim()}
          <div class="px-2 py-1 text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
            Recent
          </div>
        {/if}
        {#each searchHits as hit, i (`${hit.kind}-${hit.label}-${i}`)}
          <button
            type="button"
            onclick={hit.onSelect}
            oncontextmenu={(e) => hit.track && contextMenu.showForTrack(e, hit.track)}
            class="flex w-full cursor-pointer items-center gap-2.5 rounded-[7px] px-2 py-1.5 text-left transition-colors {selectedIdx === i
              ? 'bg-[var(--lw-wash-pressed)]'
              : 'hover:bg-[var(--lw-wash-hover)]'}"
          >
            {#if hit.kind === 'recent'}
              <WaveIcon name="clock" size={14} class="text-[var(--lw-text-tertiary)]" />
            {:else if hit.kind === 'seeAll'}
              <WaveIcon name="search" size={14} class="text-[var(--lw-text-secondary)]" />
            {:else}
              <Artwork src={hit.art} label={hit.label} size={28} radius={6} circle={Boolean(hit.circle)} />
            {/if}
            <div class="min-w-0 flex-1">
              <div class="truncate text-[12.5px] font-semibold text-[var(--lw-text-primary)]">{hit.label}</div>
              {#if hit.kind !== 'recent'}
                <div class="truncate text-[11px] text-[var(--lw-text-secondary)]">{hit.sub}</div>
              {/if}
            </div>
          </button>
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
          class="lw-popover absolute top-9 right-0 z-50 w-[236px] rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1 text-[12.5px] shadow-[var(--lw-shadow-flyout)]"
        >
          <button
            type="button"
            onclick={openNameEditor}
            class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="wand" size={14} class="text-[var(--lw-text-secondary)]" />
            <span class="truncate">{settingsStore.displayName.trim() ? `Name: ${settingsStore.displayName.trim()}` : 'Set your name…'}</span>
          </button>
          <button
            type="button"
            onclick={() => avatarFileInput?.click()}
            class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="palette" size={14} class="text-[var(--lw-text-secondary)]" />
            <span>{settingsStore.profilePictureUrl ? 'Change local picture…' : 'Choose local picture…'}</span>
          </button>
          {#if settingsStore.profilePictureUrl}
            <button
              type="button"
              onclick={removeProfilePicture}
              class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left hover:bg-[var(--lw-wash-hover)]"
            >
              <WaveIcon name="trash" size={14} class="text-[var(--lw-text-secondary)]" />
              <span>Remove local picture</span>
            </button>
          {/if}
          <div class="my-1 border-t border-[var(--lw-divider)]"></div>
          <button
            type="button"
            onclick={() => {
              profileMenuOpen = false;
              router.navigate('/settings/integrations');
            }}
            class="flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="globe" size={14} class="text-[var(--lw-text-secondary)]" />
            <span class="truncate">
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
          class="lw-popover absolute top-9 right-0 z-50 w-[196px] rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1 text-[12.5px] shadow-[var(--lw-shadow-flyout)]"
        >
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/search');
            }}
            class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <span class="flex items-center gap-2"><WaveIcon name="search" size={14} /> Search</span>
            <span class="text-[10.5px] text-[var(--lw-text-tertiary)]">Ctrl+K</span>
          </button>
          <div class="my-1 border-t border-[var(--lw-divider)]"></div>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/history');
            }}
            class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="clock" size={14} /> History
          </button>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/store');
            }}
            class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="plugin" size={14} /> Plugins & Themes
          </button>
          <div class="my-1 border-t border-[var(--lw-divider)]"></div>
          <button
            type="button"
            onclick={() => {
              menuOpen = false;
              router.navigate('/settings');
            }}
            class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
          >
            <WaveIcon name="settings" size={14} /> Settings
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

  :global(:root[data-glass="true"]) .title-bar {
    border-bottom-color: transparent;
    background: transparent;
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
