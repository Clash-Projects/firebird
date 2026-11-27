<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import { goPlaylist } from '$src/ui/navigation/waveNav';

  let spotifyPlaylists = $derived(libraryStore.playlists.filter((p) => p.source === 'spotify'));
</script>

{#if spotifyPlaylists.length > 0}
  <Shelf title="Spotify Picks" subtitle="Synced from your Spotify connection">
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
      {#each spotifyPlaylists as pl (pl.id)}
        <button
          type="button"
          onclick={() => goPlaylist(pl.id, 'spotify')}
          class="cursor-pointer rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-3 text-left hover:bg-[var(--lw-wash-hover)]"
        >
          <Artwork src={pl.artworkUrl} alt={pl.name} radius={12} class="aspect-square w-full" />
          <div class="mt-2.5 truncate text-[13.5px] font-semibold">{pl.name}</div>
          <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{pl.trackIds.length} tracks</div>
        </button>
      {/each}
    </div>
  </Shelf>
{/if}
