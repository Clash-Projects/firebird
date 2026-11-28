<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import States from '$src/ui/components/States.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let filter = $state('');
  let allTracks = $derived(libraryStore.tracksByIds(libraryStore.likedIds));
  let tracks = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return allTracks;
    return allTracks.filter(
      (t) =>
        t.title.toLowerCase().includes(q) ||
        t.artist.toLowerCase().includes(q) ||
        t.album.toLowerCase().includes(q)
    );
  });
  let likedArt = $derived(libraryStore.playlists.find((p) => p.id === 'pl-liked')?.artworkUrl ?? '');

  function shuffleLiked() {
    if (tracks.length === 0) return;
    const shuffled = [...tracks].sort(() => Math.random() - 0.5);
    playTrackNow(shuffled[0], shuffled);
  }
</script>

<div class="space-y-5 px-8 py-8 pb-10">
  <HeroHeader
    overline="Collection"
    title="Liked Songs"
    subtitle="Deduplicated across local library and YouTube Music"
    meta="{allTracks.length} tracks"
    artworkUrl={likedArt}
  >
    {#snippet actions()}
      <Buttons variant="primary" disabled={tracks.length === 0} onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
        <WaveIcon name="play" size={15} filled /> Play Liked
      </Buttons>
      <Buttons variant="secondary" disabled={tracks.length === 0} onclick={shuffleLiked}>
        <WaveIcon name="shuffle" size={14} /> Shuffle
      </Buttons>
    {/snippet}
  </HeroHeader>

  {#if allTracks.length > 0}
    <div class="max-w-[340px]">
      <Input type="search" bind:value={filter} placeholder="Filter liked songs…" />
    </div>
    <DesktopTable
      {tracks}
      removeLabel="Unlike"
      onRemove={(track) => libraryStore.toggleLike(track.id)}
    />
  {:else}
    <States icon="heart" title="No liked songs yet" description="Tap the heart icon on any track to save it here." />
  {/if}
</div>
