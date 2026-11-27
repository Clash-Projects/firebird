<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    albumId: string;
  }

  let { albumId }: Props = $props();
  let album = $derived(libraryStore.albums.find((a) => a.id === albumId) ?? libraryStore.albums[0]);
  let tracks = $derived(album ? libraryStore.tracksByIds(album.trackIds) : []);
</script>

<div class="mx-auto max-w-[1280px] px-6 py-4 pb-10">
  {#if album}
    <HeroHeader
      overline={album.type}
      title={album.title}
      subtitle={album.artist}
      meta="{album.year} · {tracks.length} tracks"
      artworkUrl={album.artworkUrl}
    >
      {#snippet actions()}
        <Buttons variant="primary" onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play Album
        </Buttons>
      {/snippet}
    </HeroHeader>
    <DesktopTable {tracks} showAlbum={false} />
  {/if}
</div>
