<script lang="ts">
  import PlaylistDetailPage from './PlaylistDetailPage.svelte';

  interface Props {
    playlistId: string;
  }

  let { playlistId }: Props = $props();
</script>

<PlaylistDetailPage {playlistId} />
