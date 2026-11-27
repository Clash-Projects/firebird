<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { windowState } from '$src/app/window.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  import { getCurrentWindow } from '@tauri-apps/api/window';
  
  const appWindow = getCurrentWindow();

  let searchQuery = $state('');
  let searchFocused = $state(false);
  let menuOpen = $state(false);

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
    if (searchQuery.trim()) libraryStore.pushSearchHistory(searchQuery.trim());
    searchFocused = false;
    router.navigate('/search');
  }
</script>

<WaveHaze
  level="l1"
  enableBlur={false}
  dataLw="title-bar"
  class="relative z-30"
>
  <div class="flex h-[44px] shrink-0 items-center border-b border-[var(--lw-divider)] bg-[var(--lw-bg)]/90 px-1.5" data-tauri-drag-region>
    <div class="flex items-center gap-0.5">
      <button
        type="button"
        onclick={() => windowState.toggleRail()}
        title="Toggle navigation"
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
    <div class="relative ml-2 w-[clamp(180px,26vw,300px)]">
      <form onsubmit={submitSearch} class="relative flex items-center">
        <WaveIcon name="search" size={14} class="pointer-events-none absolute left-2.5 text-[var(--lw-text-tertiary)]" />
        <input
          type="text"
          bind:value={searchQuery}
          onfocus={() => (searchFocused = true)}
          onblur={() => setTimeout(() => (searchFocused = false), 140)}
          placeholder="Search music..."
          class="h-[30px] w-full rounded-[10px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] pr-7 pl-8 text-[12.5px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)] focus:border-[var(--lw-accent)] focus:outline-none"
        />
        {#if searchQuery}
          <button
            type="button"
            onclick={() => (searchQuery = '')}
            class="absolute right-1.5 flex h-5 w-5 cursor-pointer items-center justify-center rounded text-[var(--lw-text-tertiary)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="close" size={12} />
          </button>
        {/if}
      </form>
      {#if searchFocused}
        <div class="absolute top-9 left-0 z-50 w-[320px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 shadow-[var(--lw-shadow-flyout)]">
          {#if !searchQuery.trim()}
            <div class="px-2 py-1 text-[10.5px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
              Recent
            </div>
            {#each libraryStore.searchHistory.slice(0, 4) as item (item)}
              <button
                type="button"
                onmousedown={() => {
                  searchQuery = item;
                  router.navigate('/search');
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
                onmousedown={() => playTrackNow(track)}
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
      {/if}
    </div>
    <button
      type="button"
      onclick={() => (windowState.commandPaletteOpen = true)}
      title="Commands (Ctrl+K)"
      class="ml-1 hidden h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)] sm:flex"
    >
      <WaveIcon name="terminal" size={15} />
    </button>
    <div class="flex-1"></div>
    <div class="flex items-center gap-1">
      <button
        type="button"
        onclick={() => router.navigate('/settings/integrations')}
        title={settingsStore.ytAccountName}
        class="flex h-7 cursor-pointer items-center gap-1.5 rounded-full border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-2 text-[11.5px] font-semibold text-[var(--lw-text-primary)] hover:bg-[var(--lw-wash-hover)]"
      >
        <span class="flex h-4 w-4 items-center justify-center rounded-full bg-[var(--lw-accent)] text-[10px] font-bold text-[var(--lw-on-accent)]">
          {settingsStore.ytAccountName[0]}
        </span>
        <span class="hidden md:inline">{settingsStore.ytAccountName}</span>
      </button>
      <div class="relative">
        <button
          type="button"
          onclick={() => (menuOpen = !menuOpen)}
          title="App menu"
          class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
        >
          <WaveIcon name="more" size={15} />
        </button>
        {#if menuOpen}
          <div class="absolute top-9 right-0 z-50 w-[190px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 text-[12.5px] shadow-[var(--lw-shadow-flyout)]">
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/settings'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="settings" size={14} /> Settings
            </button>
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/settings/plugins'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="plugin" size={14} /> Plugins & Themes
            </button>
            <button type="button" onclick={() => { menuOpen = false; windowState.updateDialogOpen = true; }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="sparkles" size={14} /> Check for updates
            </button>
            <button type="button" onclick={() => { menuOpen = false; router.navigate('/support'); }} class="flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]">
              <WaveIcon name="info" size={14} /> Keyboard shortcuts
            </button>
          </div>
        {/if}
      </div>
      <div class="ml-1 flex items-center">
        <button
          type="button"
          onclick={() => appWindow.minimize()}
          title="Mini player"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="h-[1.5px] w-2.5 bg-current"></span>
        </button>
        <button
          type="button"
          onclick={() => appWindow.toggleMaximize()}
          title="Maximize"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
        >
          <span class="h-2.5 w-2.5 rounded-[2px] border border-current"></span>
        </button>
        <button
          type="button"
          onclick={() => appWindow.close()}
          title="Close"
          class="flex h-8 w-9 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-danger)] hover:text-white"
        >
          <WaveIcon name="close" size={14} />
        </button>
      </div>
    </div>
  </div>
</WaveHaze>
