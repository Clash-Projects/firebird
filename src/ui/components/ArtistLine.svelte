<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import { goArtist } from '$src/ui/navigation/waveNav';

  interface Props {
    artists: string[];
    class?: string;
  }

  let { artists, class: className = '' }: Props = $props();

  function openArtist(name: string, e: MouseEvent) {
    e.stopPropagation();
    const match = libraryStore.artists.find((a) => a.name.toLowerCase().includes(name.toLowerCase()));
    goArtist(match?.id ?? 'art-01');
  }
</script>

<span class="truncate text-[12px] text-[var(--lw-text-secondary)] {className}">
  {#each artists as name, i (name)}
    {#if i > 0}<span>, </span>{/if}
    <button
      type="button"
      onclick={(e) => openArtist(name, e)}
      class="cursor-pointer hover:text-[var(--lw-text-primary)] hover:underline"
    >
      {name}
    </button>
  {/each}
</span>
