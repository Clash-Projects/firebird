<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { router } from '$src/app/router.svelte';
  import type { Artist, Track } from '$src/state/types';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Skeletons from '$src/ui/components/Skeletons.svelte';
  import States from '$src/ui/components/States.svelte';
  import { goArtist } from '$src/ui/navigation/waveNav';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let { artistId, artist, tracks, loading }: {
    artistId: string;
    artist: Artist | undefined;
    tracks: Track[];
    loading: boolean;
  } = $props();

  function returnToArtist() {
    if (router.history[router.index - 1] === `/artist/${artistId}`) router.back();
    else goArtist(artistId);
  }
</script>

<div data-lw="artist-popular-tracks" class="space-y-4 px-5 pt-6 pb-10 sm:px-8" aria-busy={loading}>
  <Buttons variant="ghost" size="sm" onclick={returnToArtist}>
    <WaveIcon name="chevronLeft" size={15} /> {artist?.name || 'Artist page'}
  </Buttons>

  <HeroHeader title="Popular tracks" meta={tracks.length ? `${tracks.length} ${tracks.length === 1 ? 'track' : 'tracks'}` : undefined}>
    {#snippet actions()}
      <Buttons variant="primary" disabled={!tracks.length} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
        <WaveIcon name="play" size={15} filled /> Play all
      </Buttons>
    {/snippet}
  </HeroHeader>

  {#if tracks.length}
    <DesktopTable {tracks} showAlbum={tracks.some((track) => Boolean(track.album))} />
  {:else if loading}
    <Skeletons variant="rows" />
  {:else}
    <States title="No popular tracks available" />
  {/if}
</div>
