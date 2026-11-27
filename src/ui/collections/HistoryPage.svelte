<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import DesktopTable from '$src/ui/components/DesktopTable.svelte';

  let tracks = $derived(libraryStore.tracksByIds(libraryStore.historyIds));
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Listening History" subtitle="Recently played tracks" />
  <DesktopTable {tracks} />
</div>
