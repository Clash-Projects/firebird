<script lang="ts">
  import { downloadTrack, playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore, mapYtTrack } from '$src/state/library.svelte';
  import type { Album, Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Skeletons from '$src/ui/components/Skeletons.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    albumId: string;
  }

  let { albumId }: Props = $props();
  let loading = $state(isTauriRuntime());
  let fetchedAlbum = $state<Album | null>(null);
  let fetchedTracks = $state<Track[]>([]);

  $effect(() => {
    const id = albumId;
    if (!id || !isTauriRuntime()) {
      loading = false;
      return;
    }
    let cancelled = false;
    loading = true;
    void ipcInvoke<{
      browseId: string;
      title: string;
      artist: string;
      artworkUrl: string;
      year: string;
      tracks: any[];
    } | null>('yt_browse_album', { browseId: id, limit: 100 })
      .then((res) => {
        if (cancelled || !res) return;
        const mapped = (res.tracks ?? []).map(mapYtTrack);
        libraryStore.upsertTracks(mapped);
        const alb: Album = {
          id: res.browseId || id,
          title: res.title || 'Album',
          artist: res.artist || '',
          year: Number(res.year) || 0,
          type: 'Album',
          artworkUrl: res.artworkUrl || mapped[0]?.artworkUrl || '',
          accentColor: '#7c9bff',
          trackIds: mapped.map((t) => t.id),
        };
        libraryStore.upsertAlbums([alb]);
        fetchedAlbum = alb;
        fetchedTracks = mapped;
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  let album = $derived(
    fetchedAlbum?.id === albumId
      ? fetchedAlbum
      : libraryStore.albums.find((a) => a.id === albumId)
  );
  let tracks = $derived(
    fetchedAlbum?.id === albumId && fetchedTracks.length > 0
      ? fetchedTracks
      : album
        ? libraryStore.tracksByIds(album.trackIds)
        : []
  );

  function shuffleAlbum() {
    if (tracks.length === 0) return;
    const shuffled = [...tracks].sort(() => Math.random() - 0.5);
    playTrackNow(shuffled[0], shuffled);
  }

  function downloadAlbum() {
    if (tracks.length === 0) return;
    tracks.forEach((t) => libraryStore.enqueueDownload(t));
    featuresStore.showToast({
      title: `Queued ${tracks.length} tracks for offline playback`,
      subtitle: album?.title,
      severity: 'success',
    });
  }
</script>

<div class="px-8 py-8 pb-10">
  {#if album}
    <div class="lw-enter" style:--lw-rise="4px">
      <HeroHeader
        overline={album.type}
        title={album.title}
        subtitle={album.artist}
        meta="{album.year ? `${album.year} · ` : ''}{tracks.length} tracks"
        artworkUrl={album.artworkUrl}
      >
        {#snippet actions()}
          <Buttons variant="primary" disabled={tracks.length === 0} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
            <WaveIcon name="play" size={15} filled /> Play Album
          </Buttons>
          <Buttons variant="secondary" disabled={tracks.length === 0} onclick={shuffleAlbum}>
            <WaveIcon name="shuffle" size={14} /> Shuffle
          </Buttons>
          <Buttons variant="ghost" disabled={tracks.length === 0} onclick={downloadAlbum}>
            <WaveIcon name="download" size={14} /> Save offline
          </Buttons>
        {/snippet}
      </HeroHeader>
    </div>
    {#if loading && tracks.length === 0}
      <Skeletons variant="rows" rows={8} />
    {:else}
      <div class="lw-route-enter">
        <DesktopTable {tracks} showAlbum={false} />
      </div>
    {/if}
  {:else if loading}
    <Skeletons variant="feature" featureSize={168} />
    <div class="mt-5">
      <Skeletons variant="rows" rows={8} />
    </div>
  {/if}
</div>
