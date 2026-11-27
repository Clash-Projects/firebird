<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goAlbum } from '$src/ui/navigation/waveNav';

  let filter = $state('');
  let filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return libraryStore.albums;
    return libraryStore.albums.filter(
      (a) => a.title.toLowerCase().includes(q) || a.artist.toLowerCase().includes(q)
    );
  });
</script>

<div class="space-y-5 px-8 py-8 pb-10">
  <CommandBar title="Albums" subtitle="{libraryStore.albums.length} albums in your library" />
  {#if libraryStore.albums.length > 0}
    <div class="flex max-w-[360px] items-center gap-3">
      <Input bind:value={filter} placeholder="Filter albums…" />
      <span class="shrink-0 text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{filtered.length} shown</span>
    </div>
  {/if}
  {#if filtered.length === 0}
    <States icon="album" title="No albums yet" description="Save or play albums to see them in your library." />
  {:else}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
      {#each filtered as album (album.id)}
        <button type="button" onclick={() => goAlbum(album.id)} class="group cursor-pointer rounded-[12px] p-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
          <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" alt={album.title} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
          <div class="mt-2.5 truncate text-[13px] font-semibold">{album.title}</div>
          <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{album.artist}{album.year ? ` · ${album.year}` : ''}</div>
        </button>
      {/each}
    </div>
  {/if}
</div>
