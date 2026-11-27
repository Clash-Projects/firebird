<script lang="ts">
  import { libraryStore } from '$src/state/library.svelte';
  import CommandBar from '$src/ui/components/CommandBar.svelte';
  import { goGenre } from '$src/ui/navigation/waveNav';
</script>

<div class="mx-auto max-w-[1280px] space-y-5 px-6 py-4 pb-10">
  <CommandBar title="Moods & Genres" subtitle="Browse curated radio stations and soundscapes" />
  <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
    {#each libraryStore.genres as genre (genre.id)}
      <button
        type="button"
        onclick={() => goGenre(genre.id)}
        class="lw-press flex h-32 cursor-pointer flex-col justify-end rounded-[12px] border border-[var(--lw-outline)] p-5 text-left transition-[border-color,transform] duration-[120ms] ease-[var(--ease-wave)] hover:border-[var(--lw-text-tertiary)]"
        style:background="linear-gradient(135deg, {genre.color}, #111111)"
      >
        <div class="text-[18px] font-bold text-white">{genre.name}</div>
        <div class="text-[12.5px] text-white/75">{genre.subtitle} · {genre.trackIds.length} tracks</div>
      </button>
    {/each}
  </div>
</div>
