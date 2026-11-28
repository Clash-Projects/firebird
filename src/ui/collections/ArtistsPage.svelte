<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goArtist } from '$src/ui/navigation/waveNav';

  let filter = $state('');
  let filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return libraryStore.artists;
    return libraryStore.artists.filter((a) => a.name.toLowerCase().includes(q));
  });
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Artists" subtitle="{libraryStore.artists.length} followed & library artists" />
  {#if libraryStore.artists.length > 0}
    <div class="flex max-w-[360px] items-center gap-3">
      <Input type="search" bind:value={filter} placeholder="Filter artists…" />
      <span class="shrink-0 text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">{filtered.length} shown</span>
    </div>
  {/if}
  {#if filtered.length === 0}
    <States icon="artist" title="No artists yet" description="Follow or play artists to build your artist directory." />
  {:else}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
      {#each filtered as artist (artist.id)}
        <button type="button" onclick={() => goArtist(artist.id)} class="group cursor-pointer rounded-[12px] p-3 text-center transition-colors hover:bg-[var(--lw-wash-hover)]">
          <Artwork src={artist.artworkUrl} label={artist.name} kind="artist" alt={artist.name} circle shadow="lift" class="mx-auto aspect-square w-full max-w-[156px] transition-transform duration-[180ms] group-hover:scale-[1.03]" />
          <div class="mt-3 truncate text-[14px] font-semibold">{artist.name}</div>
          <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{artist.monthlyListeners}</div>
        </button>
      {/each}
    </div>
  {/if}
</div>
