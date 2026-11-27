<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import { NOW_PLAYING_STYLES } from './nowPlayingStyle';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let open = $state(false);
</script>

<div class="relative">
  <button
    type="button"
    onclick={() => (open = !open)}
    class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
    title="Now Playing options"
  >
    <WaveIcon name="more" size={16} />
  </button>

  {#if open}
    <div class="absolute right-0 top-10 z-30 w-[230px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-2 shadow-[var(--lw-shadow-flyout)]">
      <div class="px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        Layout Style
      </div>
      {#each NOW_PLAYING_STYLES as style (style.id)}
        <button
          type="button"
          onclick={() => {
            settingsStore.nowPlayingStyle = style.id;
            open = false;
          }}
          class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
        >
          <span>{style.label}</span>
          {#if settingsStore.nowPlayingStyle === style.id}
            <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
          {/if}
        </button>
      {/each}

      <div class="my-1.5 h-px bg-[var(--lw-divider)]"></div>

      <div class="px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        Playback Speed ({playerStore.speed.toFixed(2)}x)
      </div>
      <div class="flex gap-1 px-2 py-1">
        {#each [0.75, 1.0, 1.25, 1.5] as spd (spd)}
          <button
            type="button"
            onclick={() => (playerStore.speed = spd)}
            class="flex-1 cursor-pointer rounded-[6px] py-1 text-[11px] font-semibold {playerStore.speed === spd
              ? 'bg-[var(--lw-accent)] text-[var(--lw-on-accent)]'
              : 'bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]'}"
          >
            {spd}x
          </button>
        {/each}
      </div>

      <button
        type="button"
        onclick={() => {
          windowState.toggleRightPanel('canvas');
          open = false;
        }}
        class="mt-1 flex w-full cursor-pointer items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
      >
        <WaveIcon name="layers" size={14} /> Open Canvas Panel
      </button>
    </div>
  {/if}
</div>
