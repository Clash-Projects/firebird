<script lang="ts">
  interface Props {
    rows?: number;
  }

  let { rows = 5 }: Props = $props();
</script>

<div class="space-y-2">
  {#each Array.from({ length: rows }) as _, i (i)}
    <div class="flex h-[54px] items-center gap-3 px-2.5">
      <div class="lw-skeleton h-10 w-10 rounded-[8px]"></div>
      <div class="flex-1 space-y-2">
        <div class="lw-skeleton h-3.5 w-48 rounded-[6px]"></div>
        <div class="lw-skeleton h-3 w-32 rounded-[6px]"></div>
      </div>
      <div class="lw-skeleton h-3 w-12 rounded-[6px]"></div>
    </div>
  {/each}
</div>
