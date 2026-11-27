<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtAlbum, mapYtTrack } from '$src/state/library.svelte';
  import type { Album, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Shelf from '$src/ui/components/Shelf.svelte';
  import Skeletons from '$src/ui/components/Skeletons.svelte';
  import { goAlbum } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    artistId: string;
  }

  let { artistId }: Props = $props();
  let loading = $state(false);
  let remoteSongs = $state<Track[]>([]);
  let remoteAlbums = $state<Album[]>([]);

  $effect(() => {
    const id = artistId;
    if (!id || !isTauriRuntime()) return;
    let cancelled = false;
    loading = true;
    void ipcInvoke<{
      songs?: any[];
      shelves?: Array<{ entities?: any[]; tracks?: any[] }>;
    } | null>('yt_fetch_artist_page', { browseId: id })
      .then((res) => {
        if (cancelled || !res) return;
        const songs = (res.songs ?? []).map(mapYtTrack);
        const albums: Album[] = [];
        for (const shelf of res.shelves ?? []) {
          for (const e of shelf.entities ?? []) {
            if ((e.kind || '').toLowerCase() === 'album') {
              albums.push(mapYtAlbum(e));
            }
          }
        }
        libraryStore.upsertTracks(songs);
        libraryStore.upsertAlbums(albums);
        remoteSongs = songs;
        remoteAlbums = albums;
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  let artist = $derived(libraryStore.artists.find((a) => a.id === artistId));
  let topTracks = $derived(
    remoteSongs.length > 0
      ? remoteSongs
      : artist
        ? libraryStore.tracksByIds(artist.topTrackIds)
        : []
  );
  let discography = $derived(
    remoteAlbums.length > 0
      ? remoteAlbums
      : artist
        ? libraryStore.albums.filter((al) => artist.albumIds.includes(al.id))
        : []
  );
</script>

<div class="space-y-8 px-8 py-4 pb-10">
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
        <Buttons variant="primary" disabled={topTracks.length === 0} onclick={() => topTracks[0] && playTrackNow(topTracks[0], topTracks)}>
          <WaveIcon name="play" size={15} filled /> Play Top Tracks
        </Buttons>
        <Buttons variant="secondary" disabled={topTracks.length === 0} onclick={() => { const s = [...topTracks].sort(() => Math.random() - 0.5); s[0] && playTrackNow(s[0], s); }}>
          <WaveIcon name="shuffle" size={14} /> Shuffle
        </Buttons>
        <Buttons variant="secondary" onclick={() => libraryStore.toggleFollowArtist(artist.id)}>
          {artist.isFollowing ? 'Following' : 'Follow'}
        </Buttons>
      {/snippet}
    </HeroHeader>

    {#if loading && topTracks.length === 0}
      <Skeletons variant="rows" rows={6} />
    {:else if topTracks.length > 0}
      <Shelf title="Popular">
        <DesktopTable tracks={topTracks} />
      </Shelf>
    {/if}

    {#if discography.length > 0}
      <Shelf title="Discography">
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-4 lg:grid-cols-6">
          {#each discography as album (album.id)}
            <button type="button" onclick={() => goAlbum(album.id)} class="cursor-pointer rounded-[12px] p-2 text-left transition-colors hover:bg-[var(--lw-wash-hover)]">
              <Artwork src={album.artworkUrl} label={album.title} artist={album.artist} kind="album" radius={10} class="aspect-square w-full" />
              <div class="mt-2 truncate text-[13px] font-semibold">{album.title}</div>
              <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
                {album.year ? `${album.year} · ` : ''}{album.type}
              </div>
            </button>
          {/each}
        </div>
      </Shelf>
    {/if}
  {:else if loading}
    <Skeletons variant="feature" />
    <div class="mt-6">
      <Skeletons variant="rows" rows={6} />
    </div>
  {/if}
</div>
