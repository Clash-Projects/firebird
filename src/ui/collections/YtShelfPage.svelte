<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';

  interface Props {
    title?: string;
  }

  let { title = 'YouTube Music Shelf' }: Props = $props();
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar {title} subtitle="Curated tracks from InnerTube feed" />
  <DesktopTable tracks={libraryStore.tracks} />
</div>
