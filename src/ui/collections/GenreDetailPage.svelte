<script lang="ts">
  import { playTrackNow } from '$src/app/trackActions';
  import { libraryStore } from '$src/state/library.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';
  import HeroHeader from '$src/ui/components/HeroHeader.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    genreId: string;
  }

  let { genreId }: Props = $props();
  let genre = $derived(libraryStore.genres.find((g) => g.id === genreId) ?? libraryStore.genres[0]);
  let tracks = $derived(genre ? libraryStore.tracksByIds(genre.trackIds) : []);
</script>

<div class="mx-auto max-w-[1280px] px-6 py-4 pb-10">
  {#if genre}
    <HeroHeader overline="Genre Radio" title={genre.name} subtitle={genre.subtitle} meta="{tracks.length} tracks">
      {#snippet actions()}
        <Buttons variant="primary" onclick={() => tracks[0] && playTrackNow(tracks[0], tracks)}>
          <WaveIcon name="play" size={15} filled /> Play Station
        </Buttons>
      {/snippet}
    </HeroHeader>
    <DesktopTable {tracks} />
  {/if}
</div>
