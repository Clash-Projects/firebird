<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtTrack } from '$src/state/library.svelte';
  import type { Album, Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    albumId: string;
  }

  let { albumId }: Props = $props();
  let loading = $state(false);
  let fetchedAlbum = $state<Album | null>(null);
  let fetchedTracks = $state<Track[]>([]);

  $effect(() => {
    const id = albumId;
    if (!id || !isTauriRuntime()) return;
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
</script>

<div class="px-8 py-8 pb-10">
  {#if album}
    <HeroHeader
      overline={album.type}
      title={album.title}
      subtitle={album.artist}
      meta="{album.year ? `${album.year} · ` : ''}{tracks.length} tracks"
      artworkUrl={album.artworkUrl}
    >
      {#snippet actions()}
        <Buttons variant="primary" onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play Album
        </Buttons>
      {/snippet}
    </HeroHeader>
    {#if loading && tracks.length === 0}
      <div class="py-8 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading album tracks...</div>
    {:else}
      <DesktopTable {tracks} showAlbum={false} />
    {/if}
  {:else if loading}
    <div class="py-12 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading album...</div>
  {/if}
</div>
