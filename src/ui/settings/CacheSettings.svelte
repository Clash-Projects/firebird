<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import Buttons from '$src/ui/components/Buttons.svelte';

  interface GroupStat {
    entries: number;
    chars: number;
  }

  const GROUP_LABELS: Record<string, string> = {
    home: 'Home',
    artist: 'Artist pages',
    album: 'Albums',
    playlist: 'Playlists',
    library: 'Library',
    lyrics: 'Lyrics',
    lastfm: 'Scrobbler',
    genre: 'Genres',
  };

  let stats = $state<Record<string, GroupStat>>({});

  function formatSize(chars: number): string {
    if (chars < 1024) return `${chars} B`;
    if (chars < 1024 * 1024) return `${Math.round(chars / 1024)} KB`;
    return `${(chars / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function refresh() {
    if (!isTauriRuntime()) return;
    const raw = await ipcInvoke<Record<string, GroupStat>>('library_snapshot_cache_stats').catch(
      () => null
    );
    stats = raw && typeof raw === 'object' ? raw : {};
  }

  onMount(() => {
    void refresh();
  });

  async function clearCache() {
    if (!isTauriRuntime()) return;
    await ipcInvoke('library_snapshot_cache_clear', { group: null }).catch(() => null);
    await refresh();
  }

  let groupKeys = $derived(Object.keys(stats).sort());
  let totalEntries = $derived(groupKeys.reduce((acc, k) => acc + (stats[k]?.entries ?? 0), 0));
  let totalChars = $derived(groupKeys.reduce((acc, k) => acc + (stats[k]?.chars ?? 0), 0));
</script>

<div>
  <div class="text-[13px] text-[var(--lw-text-primary)] lw-tabular">
    {totalEntries === 0 ? 'Nothing cached yet.' : `${totalEntries} saved items · ${formatSize(totalChars)}`}
  </div>

  {#if groupKeys.length > 0}
    <div class="mt-1.5 space-y-0.5">
      {#each groupKeys as g (g)}
        <div class="text-[12px] text-[var(--lw-text-secondary)] lw-tabular">
          {GROUP_LABELS[g] ?? g}: {stats[g].entries} ({formatSize(stats[g].chars)})
        </div>
      {/each}
    </div>
  {/if}

  <div class="mt-2.5 flex items-center">
    <Buttons variant="secondary" size="sm" disabled={totalEntries === 0} onclick={() => void clearCache()}>
      Clear cached data
    </Buttons>
  </div>

  <p class="mt-2 text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
    Saved copies of pages so they open instantly and refresh in the background. Clearing them only makes the next open slower once. Downloads, artwork and your library are not affected.
  </p>
</div>
