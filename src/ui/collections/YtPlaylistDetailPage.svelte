<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { libraryStore, mapYtTrack } from '$src/state/library.svelte';
  import type { Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    playlistId: string;
  }

  let { playlistId }: Props = $props();
  let loading = $state(false);
  let title = $state('');
  let author = $state('');
  let artworkUrl = $state('');
  let tracks = $state<Track[]>([]);

  $effect(() => {
    const id = playlistId;
    if (!id || !isTauriRuntime()) return;
    let cancelled = false;
    loading = true;
    void ipcInvoke<{
      id: string;
      title: string;
      author: string;
      artworkUrl: string;
      tracks: any[];
    } | null>('yt_fetch_playlist', { playlistIdOrUrl: id, maxTracks: 200 })
      .then((res) => {
        if (cancelled || !res) return;
        const mapped = (res.tracks ?? []).map(mapYtTrack);
        libraryStore.upsertTracks(mapped);
        title = res.title || 'Playlist';
        author = res.author || 'YouTube Music';
        artworkUrl = res.artworkUrl || mapped[0]?.artworkUrl || '';
        tracks = mapped;
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="px-8 py-8 pb-10">
  {#if loading && tracks.length === 0}
    <div class="py-12 text-center text-[13px] text-[var(--lw-text-secondary)]">Loading playlist...</div>
  {:else}
    <HeroHeader
      overline="YouTube Music Playlist"
      title={title || 'Playlist'}
      subtitle={author}
      meta="{tracks.length} tracks"
      {artworkUrl}
    >
      {#snippet actions()}
        <Buttons variant="primary" onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play
        </Buttons>
      {/snippet}
    </HeroHeader>
    <DesktopTable {tracks} />
  {/if}
</div>
