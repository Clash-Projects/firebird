<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import { goAlbum } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    artistId: string;
  }

  let { artistId }: Props = $props();
  let artist = $derived(libraryStore.artists.find((a) => a.id === artistId) ?? libraryStore.artists[0]);
  let topTracks = $derived(artist ? libraryStore.tracksByIds(artist.topTrackIds) : []);
  let discography = $derived(
    artist ? libraryStore.albums.filter((al) => artist.albumIds.includes(al.id)) : []
  );
</script>

<div class="mx-auto max-w-[1280px] space-y-8 px-6 py-4 pb-10">
  {#if artist}
    <HeroHeader
      overline="Artist"
      title={artist.name}
      subtitle={artist.bio}
      meta={artist.monthlyListeners}
      artworkUrl={artist.artworkUrl}
      circleArt
    >
      {#snippet actions()}
        <Buttons variant="primary" onclick={() => topTracks[0] && playTrackNow(topTracks[0], topTracks)}>
          <WaveIcon name="play" size={15} filled /> Play Top Tracks
        </Buttons>
        <Buttons variant="secondary" onclick={() => libraryStore.toggleFollowArtist(artist.id)}>
          {artist.isFollowing ? 'Following' : 'Follow'}
        </Buttons>
      {/snippet}
    </HeroHeader>

    <Shelf title="Popular">
      <DesktopTable tracks={topTracks} />
    </Shelf>

    {#if discography.length > 0}
      <Shelf title="Discography">
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
          {#each discography as album (album.id)}
            <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[14px] p-2 text-left hover:bg-[var(--lw-wash-hover)]">
              <Artwork src={album.artworkUrl} radius={12} class="aspect-square w-full" />
              <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
              <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{album.year} · {album.type}</div>
            </button>
          {/each}
        </div>
      </Shelf>
    {/if}
  {/if}
</div>
