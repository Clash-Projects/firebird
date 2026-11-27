<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goPlaylist } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import PlaylistDialogs from './PlaylistDialogs.svelte';

  let createOpen = $state(false);
  let filter = $state('');

  let q = $derived(filter.trim().toLowerCase());
  let localLists = $derived(
    libraryStore.playlists.filter(
      (p) => p.source === 'local' && p.id !== 'pl-liked' && (!q || p.name.toLowerCase().includes(q))
    )
  );
  let ytLists = $derived(
    libraryStore.playlists.filter(
      (p) => (p.source === 'youtube' || p.id === 'pl-liked') && (!q || p.name.toLowerCase().includes(q))
    )
  );
  let spotifyLists = $derived(
    libraryStore.playlists.filter(
      (p) => p.source === 'spotify' && (!q || p.name.toLowerCase().includes(q))
    )
  );
  let heroCover = $derived(localLists[0]?.artworkUrl ?? libraryStore.playlists[0]?.artworkUrl ?? '');
</script>

<div class="w-full space-y-6 px-8 py-8 pb-12">
  <HeroHeader
    overline="Collect"
    title="Playlists"
    meta="{localLists.length + ytLists.length + spotifyLists.length} playlists"
    artworkUrl={heroCover}
  >
    {#snippet actions()}
      <Buttons variant="primary" size="sm" onclick={() => (createOpen = true)}>
        <WaveIcon name="plus" size={14} /> New
      </Buttons>
    {/snippet}
  </HeroHeader>

  <div class="flex max-w-[360px] items-center gap-3">
    <Input bind:value={filter} placeholder="Filter playlists…" />
    <span class="shrink-0 text-[12px] text-[var(--lw-text-tertiary)] lw-tabular">
      {localLists.length + ytLists.length + spotifyLists.length} shown
    </span>
  </div>

  {#if localLists.length + ytLists.length + spotifyLists.length === 0}
    <States icon="library" title="No playlists yet" description="Create a new playlist to organize your music." />
  {/if}

  {#if ytLists.length > 0}
    <section class="space-y-2.5">
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">YOUTUBE MUSIC</div>
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each ytLists as pl (pl.id)}
          <button
            type="button"
            onclick={() => (pl.id === 'pl-liked' ? router.navigate('/liked') : goPlaylist(pl.id, 'youtube'))}
            class="group cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
          >
            <Artwork src={pl.artworkUrl} label={pl.name} kind="album" alt={pl.name} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{pl.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{pl.trackIds.length} tracks</div>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if localLists.length > 0}
    <section class="space-y-2.5">
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">YOUR PLAYLISTS</div>
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each localLists as pl (pl.id)}
          <button
            type="button"
            onclick={() => goPlaylist(pl.id, 'local')}
            class="group cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
          >
            <Artwork src={pl.artworkUrl} label={pl.name} kind="album" alt={pl.name} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{pl.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{pl.trackIds.length} tracks</div>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if spotifyLists.length > 0}
    <section class="space-y-2.5">
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">SPOTIFY</div>
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        {#each spotifyLists as pl (pl.id)}
          <button
            type="button"
            onclick={() => goPlaylist(pl.id, 'spotify')}
            class="group cursor-pointer rounded-[12px] p-1.5 text-left transition-colors hover:bg-[var(--lw-wash-hover)]"
          >
            <Artwork src={pl.artworkUrl} label={pl.name} kind="album" alt={pl.name} radius={10} shadow="lift" class="aspect-square w-full transition-transform duration-[180ms] group-hover:scale-[1.03]" />
            <div class="mt-2 truncate text-[13px] font-semibold">{pl.name}</div>
            <div class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">{pl.trackIds.length} tracks</div>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>

<PlaylistDialogs open={createOpen} onClose={() => (createOpen = false)} />
