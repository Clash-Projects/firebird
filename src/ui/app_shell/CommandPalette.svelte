<script lang="ts">
  import { tick } from 'svelte';
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore } from '$src/state/library.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

  interface CommandEntry {
    id: string;
    group: 'Commands' | 'Tracks';
    label: string;
    subtitle?: string;
    icon?: WaveIconName;
    art?: string;
    action: () => void;
  }

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let query = $state('');
  let selectedIndex = $state(0);
  let backdropPressed = false;
  let onlineTracks = $state<typeof libraryStore.tracks>([]);
  let searchSeq = 0;

  $effect(() => {
    if (!dialogEl) return;
    if (windowState.commandPaletteOpen && !dialogEl.open) {
      query = '';
      selectedIndex = 0;
      onlineTracks = [];
      backdropPressed = false;
      dialogEl.showModal();
    } else if (!windowState.commandPaletteOpen && dialogEl.open) {
      dialogEl.close();
    }
  });

  // Debounced live catalog lookup when typing >= 2 chars
  $effect(() => {
    if (!windowState.commandPaletteOpen || !isTauriRuntime()) return;
    const q = query.trim();
    const seq = ++searchSeq;
    if (q.length < 2) {
      onlineTracks = [];
      return;
    }
    const timer = setTimeout(() => {
      void ipcInvoke<Array<{ videoId: string; title: string; artist: string; album?: string; artworkUrl?: string; durationSeconds?: number }>>(
        'yt_search_songs',
        { query: q, limit: 6 }
      )
        .then((rows) => {
          if (seq !== searchSeq || !Array.isArray(rows)) return;
          onlineTracks = rows.map((r) => ({
            id: r.videoId || `${r.title}-${r.artist}`,
            videoId: r.videoId,
            title: r.title,
            artist: r.artist,
            artists: r.artist ? r.artist.split(/,\s*/).filter(Boolean) : ['Unknown Artist'],
            album: r.album || '',
            durationSec: Math.max(0, Number(r.durationSeconds) || 0),
            artworkUrl: r.artworkUrl || '',
            accentColor: '#7c9bff',
            qualityTier: 'high',
            sampleRateHz: 48000,
            bitDepth: 16,
            codec: 'OPUS',
            bitrateKbps: 256,
          }));
        })
        .catch(() => {});
    }, 200);
    return () => clearTimeout(timer);
  });

  const quickActions: Array<{ label: string; icon: WaveIconName; action: () => void }> = [
    { label: 'Go to Home', icon: 'home', action: () => router.navigate('/home') },
    { label: 'Go to Search', icon: 'search', action: () => router.navigate('/search') },
    { label: 'Go to Liked Songs', icon: 'heart', action: () => router.navigate('/liked') },
    { label: 'Go to Albums', icon: 'music', action: () => router.navigate('/albums') },
    { label: 'Go to Artists', icon: 'contact', action: () => router.navigate('/artists') },
    { label: 'Go to Playlists', icon: 'playlists', action: () => router.navigate('/playlists') },
    { label: 'Go to Podcasts', icon: 'podcast', action: () => router.navigate('/podcasts') },
    { label: 'Go to Library', icon: 'library', action: () => router.navigate('/library') },
    { label: 'Go to Downloads', icon: 'download', action: () => router.navigate('/downloads') },
    { label: 'Go to History', icon: 'history', action: () => router.navigate('/history') },
    { label: 'Open Now Playing', icon: 'expand', action: () => router.navigate('/now-playing') },
    { label: 'Open Stats', icon: 'stats', action: () => router.navigate('/stats') },
    { label: 'Open Settings', icon: 'settings', action: () => router.navigate('/settings') },
    { label: 'Open Equalizer', icon: 'eq', action: () => windowState.toggleRightPanel('eq') },
    { label: 'Open Audio Output', icon: 'speaker', action: () => windowState.toggleRightPanel('output') },
    { label: 'Open Plugins & Themes', icon: 'store', action: () => router.navigate('/store') },
    { label: 'Toggle Play / Pause', icon: 'play', action: () => playerStore.togglePlay() },
    { label: 'Toggle Mini Player', icon: 'miniPlayer', action: () => windowState.toggleMiniPlayer() },
  ];

  let matchedActions = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q
      ? quickActions.filter((a) => a.label.toLowerCase().includes(q))
      : quickActions.slice(0, 8);
  });

  let matchedTracks = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const local = q
      ? libraryStore.tracks.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            t.artist.toLowerCase().includes(q)
        )
      : libraryStore.tracks.slice(0, 4);

    if (!q || onlineTracks.length === 0) return local.slice(0, 8);
    const seen = new Set(local.map((t) => `${t.title.toLowerCase()}|${t.artist.toLowerCase()}`));
    const merged = [...local];
    for (const t of onlineTracks) {
      const k = `${t.title.toLowerCase()}|${t.artist.toLowerCase()}`;
      if (!seen.has(k)) {
        seen.add(k);
        merged.push(t);
      }
    }
    return merged.slice(0, 10);
  });

  let flatEntries = $derived<CommandEntry[]>([
    ...matchedActions.map((a, idx) => ({
      id: `cmd-${idx}-${a.label}`,
      group: 'Commands' as const,
      label: a.label,
      icon: a.icon,
      action: a.action,
    })),
    ...matchedTracks.map((t, idx) => ({
      id: `trk-${idx}-${t.id}`,
      group: 'Tracks' as const,
      label: t.title,
      subtitle: `${t.artist}${t.codec ? ` · ${t.codec}` : ''}`,
      art: t.artworkUrl,
      action: () => playTrackNow(t),
    })),
  ]);

  $effect(() => {
    if (selectedIndex >= flatEntries.length) {
      selectedIndex = Math.max(0, flatEntries.length - 1);
    }
  });

  function scrollSelectedIntoView() {
    void tick().then(() => {
      listEl
        ?.querySelector<HTMLElement>('[data-palette-selected="true"]')
        ?.scrollIntoView({ block: 'nearest' });
    });
  }

  function close() {
    windowState.commandPaletteOpen = false;
  }

  function isBackdrop(event: MouseEvent) {
    if (!dialogEl || event.target !== dialogEl) return false;
    const { left, right, top, bottom } = dialogEl.getBoundingClientRect();
    return event.clientX < left || event.clientX >= right || event.clientY < top || event.clientY >= bottom;
  }

  function run(fn: () => void) {
    fn();
    close();
  }

  function onInputKeydown(event: KeyboardEvent) {
    if (flatEntries.length === 0) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      selectedIndex = (selectedIndex + 1) % flatEntries.length;
      scrollSelectedIntoView();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      selectedIndex = (selectedIndex - 1 + flatEntries.length) % flatEntries.length;
      scrollSelectedIntoView();
    } else if (event.key === 'Home') {
      event.preventDefault();
      selectedIndex = 0;
      scrollSelectedIntoView();
    } else if (event.key === 'End') {
      event.preventDefault();
      selectedIndex = flatEntries.length - 1;
      scrollSelectedIntoView();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const target = flatEntries[selectedIndex];
      if (target) run(target.action);
    }
  }
</script>

<dialog
  bind:this={dialogEl}
  closedby="any"
  data-lw="command-palette"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onpointerdown={(event) => (backdropPressed = event.button === 0 && isBackdrop(event))}
  onpointercancel={() => (backdropPressed = false)}
  onclick={(event) => {
    if (backdropPressed && isBackdrop(event)) close();
    backdropPressed = false;
  }}
  aria-label="Command palette"
  class="command-palette"
>
  <div class="palette-search flex shrink-0 items-center gap-2.5 border-b border-[var(--lw-divider)] px-4 py-3">
    <WaveIcon name="search" size={15} class="text-[var(--lw-text-tertiary)]" />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="text"
      autofocus
      bind:value={query}
      oninput={() => (selectedIndex = 0)}
      onkeydown={onInputKeydown}
      aria-label="Search commands and tracks"
      placeholder="Jump to track, page, or command..."
      class="palette-input min-w-0 flex-1 bg-transparent text-[13.5px] text-[var(--lw-text-primary)] placeholder:text-[var(--lw-text-tertiary)]"
    />
    <button
      type="button"
      onclick={close}
      class="cursor-pointer rounded-[6px] px-1.5 py-0.5 text-[11px] font-medium text-[var(--lw-text-tertiary)] hover:bg-[var(--lw-wash-hover)]"
    >
      ESC
    </button>
  </div>

  <div bind:this={listEl} use:waveSmoothScroll class="lw-scroll min-h-0 max-h-[340px] p-1.5">
    {#if flatEntries.length === 0}
      <div class="px-3 py-6 text-center text-[12.5px] text-[var(--lw-text-tertiary)]">
        No matching commands or tracks.
      </div>
    {:else}
      {#each flatEntries as entry, i (entry.id)}
        {#if i === 0 || flatEntries[i - 1].group !== entry.group}
          <div class="px-2.5 pt-2 pb-1 text-[9.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
            {entry.group}
          </div>
        {/if}
        {@const active = i === selectedIndex}
        <button
          type="button"
          data-palette-selected={active}
          onmouseenter={() => (selectedIndex = i)}
          onclick={() => run(entry.action)}
          class="relative flex w-full cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-1.5 text-left transition-colors duration-[100ms] {active
            ? 'bg-[var(--lw-wash-hover)] text-[var(--lw-text-primary)]'
            : 'text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]'}"
        >
          {#if active}
            <span class="h-3.5 w-[2.5px] shrink-0 rounded-full bg-[var(--lw-accent)]"></span>
          {:else}
            <span class="w-[2.5px] shrink-0"></span>
          {/if}

          {#if entry.group === 'Tracks'}
            <Artwork src={entry.art ?? ''} label={entry.label} size={30} radius={6} />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-medium text-[var(--lw-text-primary)]">{entry.label}</div>
              {#if entry.subtitle}
                <div class="truncate text-[11px] text-[var(--lw-text-tertiary)]">{entry.subtitle}</div>
              {/if}
            </div>
          {:else}
            <WaveIcon name={entry.icon ?? 'terminal'} size={14} class={active ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'} />
            <span class="flex-1 truncate text-[13px]">{entry.label}</span>
          {/if}
        </button>
      {/each}
    {/if}
  </div>
</dialog>

<style>
  .command-palette {
    --lw-dialog-entry-transform: scale(calc(1 - 0.04 * var(--lw-motion-factor)));
    position: fixed;
    inset: 0;
    width: min(460px, calc(100% - 32px));
    height: fit-content;
    margin: auto;
    overflow: hidden;
    flex-direction: column;
    border-radius: var(--lw-radius-floating);
    border: 1px solid var(--lw-outline);
    background: var(--lw-surface-raised);
    color: var(--lw-text-primary);
    box-shadow: var(--lw-shadow-modal);
  }

  .command-palette[open] { display: flex; }
  .palette-input:focus { outline: none; }
  .palette-search:has(.palette-input:focus) { background: var(--lw-wash-hover); }
</style>
