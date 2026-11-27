<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    playlistId: string;
  }

  let { playlistId }: Props = $props();
  let playlist = $derived(libraryStore.playlists.find((p) => p.id === playlistId) ?? libraryStore.playlists[0]);
  let tracks = $derived(playlist ? libraryStore.tracksByIds(playlist.trackIds) : []);
</script>

<div class="px-8 py-8 pb-10">
  {#if playlist}
    <HeroHeader
      overline="Playlist"
      title={playlist.name}
      subtitle={playlist.description}
      meta="{tracks.length} tracks · {playlist.owner} · Updated {playlist.updatedAt}"
      artworkUrl={playlist.artworkUrl}
    >
      {#snippet actions()}
        <Buttons variant="primary" disabled={tracks.length === 0} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play All
        </Buttons>
        <Buttons variant="secondary" onclick={() => libraryStore.togglePinPlaylist(playlist.id)}>
          <WaveIcon name="pin" size={14} /> {playlist.pinned ? 'Pinned' : 'Pin'}
        </Buttons>
      {/snippet}
    </HeroHeader>

    {#if tracks.length > 0}
      <DesktopTable {tracks} />
    {:else}
      <States icon="queue" title="Playlist is empty" description="Right-click any track to add it to this playlist." />
    {/if}
  {/if}
</div>
