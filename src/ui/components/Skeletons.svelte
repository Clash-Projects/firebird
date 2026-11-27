<script lang="ts">
  interface Props {
    variant?: 'rows' | 'rail' | 'feature';
    rows?: number;
  }

  let { variant = 'rows', rows = 6 }: Props = $props();
</script>

{#if variant === 'feature'}
  <div class="flex flex-col gap-6 sm:flex-row sm:items-start" aria-busy="true">
    <div class="lw-skeleton h-[220px] w-[220px] shrink-0 rounded-[12px]"></div>
    <div class="flex-1 space-y-2.5 pt-1">
      <div class="lw-skeleton h-3 w-28 rounded-[6px]"></div>
      <div class="lw-skeleton h-7 w-64 max-w-full rounded-[6px]"></div>
      <div class="lw-skeleton h-3.5 w-48 max-w-full rounded-[6px]"></div>
      <div class="pt-2">
        <div class="lw-skeleton h-9 w-40 rounded-[10px]"></div>
      </div>
    </div>
  </div>
{:else if variant === 'rail'}
  <div class="flex gap-4 overflow-hidden" aria-busy="true">
    {#each Array.from({ length: rows }) as _, i (i)}
      <div class="w-[168px] shrink-0 space-y-2">
        <div class="lw-skeleton h-[168px] w-[168px] rounded-[12px]"></div>
        <div class="lw-skeleton h-3 w-[120px] rounded-[6px]"></div>
        <div class="lw-skeleton h-2.5 w-[90px] rounded-[6px]"></div>
      </div>
    {/each}
  </div>
{:else}
  <div class="space-y-1.5" aria-busy="true">
    {#each Array.from({ length: rows }) as _, i (i)}
      <div class="flex h-[54px] items-center gap-3 px-2.5">
        <div class="lw-skeleton h-3 w-4 rounded-[6px]"></div>
        <div class="lw-skeleton h-10 w-10 rounded-[12px]"></div>
        <div class="flex-1 space-y-1.5">
          <div class="lw-skeleton h-3 w-48 max-w-full rounded-[6px]"></div>
          <div class="lw-skeleton h-2.5 w-32 max-w-full rounded-[6px]"></div>
        </div>
        <div class="lw-skeleton h-2.5 w-12 rounded-[6px]"></div>
      </div>
    {/each}
  </div>
{/if}
