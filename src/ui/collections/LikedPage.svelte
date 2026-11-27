<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let tracks = $derived(libraryStore.tracksByIds(libraryStore.likedIds));
  let likedArt = $derived(libraryStore.playlists.find((p) => p.id === 'pl-liked')?.artworkUrl ?? '');
</script>

<div class="mx-auto max-w-[1280px] px-6 py-4 pb-10">
  <HeroHeader
    overline="Collection"
    title="Liked Songs"
    subtitle="Deduplicated across local library and YouTube Music"
    meta="{tracks.length} tracks"
    artworkUrl={likedArt}
  >
    {#snippet actions()}
      <Buttons variant="primary" disabled={tracks.length === 0} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
        <WaveIcon name="play" size={15} filled /> Play Liked
      </Buttons>
    {/snippet}
  </HeroHeader>

  {#if tracks.length > 0}
    <DesktopTable {tracks} />
  {:else}
    <States icon="heart" title="No liked songs yet" description="Tap the heart icon on any track to save it here." />
  {/if}
</div>
