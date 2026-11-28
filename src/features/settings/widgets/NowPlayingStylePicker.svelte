<script lang="ts">
  import { settingsStore } from '$src/state/settings.svelte';
  import { NOW_PLAYING_STYLES } from '$src/ui/now_playing/nowPlayingStyle';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  function selectStyle(id: (typeof NOW_PLAYING_STYLES)[number]['id']) {
    settingsStore.nowPlayingStyle = id;
    settingsStore.saveStringPref('lw_now_playing_style', id);
  }
</script>

<div class="grid grid-cols-1 gap-3.5 sm:grid-cols-3">
  {#each NOW_PLAYING_STYLES as style (style.id)}
    {@const selected = settingsStore.nowPlayingStyle === style.id}
    <button
      type="button"
      onclick={() => selectStyle(style.id)}
      aria-pressed={selected}
      class="group flex cursor-pointer flex-col text-left focus:outline-none"
    >
      <!-- 16:10 schematic miniature matching now_playing_style_picker.dart -->
      <div
        class="relative aspect-[16/10] w-full overflow-hidden rounded-[10px] border transition-colors duration-150 {selected
          ? 'border-[1.5px] border-[var(--lw-accent)]'
          : 'border-[var(--lw-divider)] group-hover:border-[var(--lw-accent)]/50'}"
      >
        {#if style.id === 'fullBleed'}
          <!-- 1. Full Bleed schematic: continuous edge-to-edge ambient canvas melting into right lyrics -->
          <div class="np-mini-fullbleed relative h-full w-full">
            <div class="np-mini-aura pointer-events-none absolute -top-3 right-[44%] -bottom-3 -left-2.5"></div>
            <div class="absolute top-1/2 left-[22%] flex h-[22px] w-[22px] -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border border-white/18 bg-white/10 text-white/70">
              <WaveIcon name="music" size={11} />
            </div>
            <!-- Top-right action dots -->
            <div class="absolute top-1.5 right-1.5 flex items-center gap-[3px]">
              <span class="h-[7px] w-[7px] rounded-[2px] bg-white/16"></span>
              <span class="h-[7.5px] w-[7.5px] rounded-full bg-white/22"></span>
            </div>
            <!-- Right lyrics bars -->
            <div class="absolute top-4 right-2.5 bottom-3.5 left-[54%] flex flex-col justify-center gap-[5px]">
              <span class="h-[3px] w-[70%] rounded-full bg-white/22"></span>
              <span class="h-[4px] w-[95%] rounded-full bg-white/92"></span>
              <span class="h-[3px] w-[78%] rounded-full bg-white/34"></span>
              <span class="h-[3px] w-[62%] rounded-full bg-white/20"></span>
            </div>
          </div>
        {:else if style.id === 'minimal'}
          <!-- 2. Minimal schematic: sharp square cover + centered meta on left, alternating duet lyrics on right -->
          <div class="np-mini-minimal relative h-full w-full">
            <span class="absolute top-1.5 left-2 h-[2px] w-[6px] rounded-full bg-white/35"></span>
            <div class="absolute top-1.5 right-1.5 flex items-center gap-[2.5px]">
              <span class="np-mini-accent-dot h-[7px] w-[7px] rounded-[2px]"></span>
              <span class="h-[7px] w-[7px] rounded-[2px] bg-white/16"></span>
              <span class="h-[7.5px] w-[7.5px] rounded-full bg-white/22"></span>
            </div>
            <div class="flex h-full items-center gap-3 px-3 pt-4 pb-2.5">
              <div class="flex h-full w-[42%] flex-col items-center justify-center">
                <div class="np-mini-cover flex aspect-square max-h-[58px] w-auto flex-1 items-center justify-center rounded-[2px] border border-white/12 text-white/55">
                  <WaveIcon name="music" size={14} />
                </div>
                <span class="mt-1.5 h-[3.5px] w-[42px] rounded-full bg-white/85"></span>
                <span class="mt-[3px] h-[2.5px] w-[26px] rounded-full bg-white/35"></span>
              </div>
              <div class="flex flex-1 flex-col justify-center gap-[5px]">
                <span class="ml-auto h-[3px] w-[64%] rounded-full bg-white/24"></span>
                <span class="ml-auto h-[4px] w-[78%] rounded-full bg-white/92"></span>
                <span class="h-[3px] w-[72%] rounded-full bg-white/36"></span>
                <span class="h-[3px] w-[56%] rounded-full bg-white/22"></span>
              </div>
            </div>
          </div>
        {:else}
          <!-- 3. Old (classic dual-pane) schematic: rounded cover + left-aligned meta & up-next on left, lyrics on right -->
          <div class="np-mini-old relative h-full w-full">
            <div class="absolute top-1.5 right-1.5 flex items-center gap-[2.5px]">
              <span class="np-mini-accent-dot h-[7px] w-[7px] rounded-[2px]"></span>
              <span class="h-[7px] w-[7px] rounded-[2px] bg-white/16"></span>
              <span class="h-[7.5px] w-[7.5px] rounded-full bg-white/22"></span>
            </div>
            <div class="flex h-full items-center gap-2.5 px-2.5 pt-3.5 pb-2">
              <div class="flex h-full w-[42%] flex-col items-start justify-center">
                <div class="np-mini-cover flex aspect-square max-h-[50px] w-auto flex-1 items-center justify-center rounded-[6px] border border-white/12 text-white/55">
                  <WaveIcon name="music" size={14} />
                </div>
                <span class="mt-1 h-[3.5px] w-[46px] rounded-full bg-white/85"></span>
                <span class="mt-[3px] h-[2.5px] w-[30px] rounded-full bg-white/42"></span>
                <div class="mt-1 flex items-center gap-[3px]">
                  <span class="h-[4px] w-[4px] rounded-full bg-[var(--lw-accent)]"></span>
                  <span class="h-[2px] w-[22px] rounded-full bg-white/30"></span>
                </div>
                <span class="mt-1 h-[2.5px] w-[50px] rounded-full bg-white/18"></span>
              </div>
              <div class="flex flex-1 flex-col justify-center gap-[5px]">
                <span class="h-[3px] w-[72%] rounded-full bg-white/24"></span>
                <span class="h-[4px] w-[96%] rounded-full bg-white/92"></span>
                <span class="h-[3px] w-[82%] rounded-full bg-white/34"></span>
                <span class="h-[3px] w-[64%] rounded-full bg-white/22"></span>
              </div>
            </div>
          </div>
        {/if}

        {#if selected}
          <span class="absolute right-1.5 bottom-1.5 flex h-[18px] w-[18px] items-center justify-center rounded-full bg-[var(--lw-accent)] text-[var(--lw-on-accent)]">
            <WaveIcon name="check" size={10} />
          </span>
        {/if}
      </div>

      <div class="mt-2 truncate text-[13px] font-semibold text-[var(--lw-text-primary)]">
        {style.label}
      </div>
      <div class="mt-0.5 line-clamp-2 text-[11px] leading-[1.28] text-[var(--lw-text-tertiary)]">
        {style.description}
      </div>
    </button>
  {/each}
</div>

<style>
  .np-mini-fullbleed {
    background: linear-gradient(
      90deg,
      color-mix(in srgb, #1a1d26 54%, var(--lw-accent)) 0%,
      color-mix(in srgb, #181b24 68%, var(--lw-accent)) 34%,
      color-mix(in srgb, #141720 85%, var(--lw-accent)) 58%,
      #12151e 82%,
      #10131b 100%
    );
  }

  .np-mini-aura {
    background: radial-gradient(
      circle at 32% 50%,
      color-mix(in srgb, var(--lw-accent) 32%, transparent) 0%,
      color-mix(in srgb, var(--lw-accent) 10%, transparent) 55%,
      transparent 100%
    );
  }

  .np-mini-minimal {
    background: linear-gradient(
      135deg,
      color-mix(in srgb, #1a1e28 84%, var(--lw-accent)) 0%,
      #12151e 100%
    );
  }

  .np-mini-old {
    background: linear-gradient(
      135deg,
      color-mix(in srgb, #1b1e2a 86%, var(--lw-accent)) 0%,
      #11141c 100%
    );
  }

  .np-mini-cover {
    background: linear-gradient(
      135deg,
      color-mix(in srgb, #2a2f3d 58%, var(--lw-accent)) 0%,
      color-mix(in srgb, #1e222e 84%, var(--lw-accent)) 100%
    );
  }

  .np-mini-accent-dot {
    background: color-mix(in srgb, var(--lw-accent) 30%, transparent);
    border: 0.6px solid color-mix(in srgb, var(--lw-accent) 60%, transparent);
  }
</style>
