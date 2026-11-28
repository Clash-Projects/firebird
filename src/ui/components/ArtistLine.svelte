<script lang="ts">
  import { goArtistByName } from '$src/ui/navigation/waveNav';

  interface Props {
    artists: string[];
    class?: string;
  }

  let { artists, class: className = '' }: Props = $props();

  function openArtist(name: string, e: MouseEvent) {
    e.stopPropagation();
    goArtistByName(name);
  }
</script>

<span class="truncate text-[12px] text-[var(--lw-text-secondary)] {className}">
  {#each artists as name, i (`${name}-${i}`)}
    {#if i > 0}<span>, </span>{/if}
    <button
      type="button"
      onclick={(e) => openArtist(name, e)}
      class="cursor-pointer transition-colors duration-[120ms] hover:text-[var(--lw-text-primary)] hover:underline"
    >
      {name}
    </button>
  {/each}
</span>
