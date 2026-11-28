<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    playlistId: string;
  }

  let { playlistId }: Props = $props();
  let filter = $state('');
  let playlist = $derived(libraryStore.playlists.find((p) => p.id === playlistId) ?? libraryStore.playlists[0]);
  let allTracks = $derived(playlist ? libraryStore.tracksByIds(playlist.trackIds) : []);
  let tracks = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return allTracks;
    return allTracks.filter(
      (t) => t.title.toLowerCase().includes(q) || t.artist.toLowerCase().includes(q)
    );
  });

  function shufflePlaylist() {
    if (tracks.length === 0) return;
    const shuffled = [...tracks].sort(() => Math.random() - 0.5);
    playTrackNow(shuffled[0], shuffled);
  }
</script>

<div class="space-y-5 px-8 py-8 pb-10">
  {#if playlist}
    <HeroHeader
      overline="Playlist"
      title={playlist.name}
      subtitle={playlist.description}
      meta="{allTracks.length} tracks · {playlist.owner}{playlist.updatedAt ? ` · Updated ${playlist.updatedAt}` : ''}"
      artworkUrl={playlist.artworkUrl}
    >
      {#snippet actions()}
        <Buttons variant="primary" disabled={tracks.length === 0} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play All
        </Buttons>
        <Buttons variant="secondary" disabled={tracks.length === 0} onclick={shufflePlaylist}>
          <WaveIcon name="shuffle" size={14} /> Shuffle
        </Buttons>
        <Buttons variant="secondary" onclick={() => libraryStore.togglePinPlaylist(playlist.id)}>
          <WaveIcon name="pin" size={14} /> {playlist.pinned ? 'Pinned' : 'Pin'}
        </Buttons>
      {/snippet}
    </HeroHeader>

    {#if allTracks.length > 0}
      <div class="max-w-[340px]">
        <Input type="search" bind:value={filter} placeholder="Filter tracks…" />
      </div>
      <DesktopTable
        {tracks}
        removeLabel={playlist.source === 'local' ? 'Remove from playlist' : undefined}
        onRemove={playlist.source === 'local'
          ? (track) => libraryStore.removeTrackFromPlaylist(playlist.id, track.id)
          : undefined}
      />
    {:else}
      <States icon="queue" title="Playlist is empty" description="Right-click any track to add it to this playlist." />
    {/if}
  {/if}
</div>
