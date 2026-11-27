<script lang="ts">
  import { dismissFlyout } from '$src/app/accessibility';
  import {
    featuresStore,
    LYRICS_ANIMATION_OPTIONS,
    LYRICS_PROVIDERS,
    LYRICS_TEXT_SCALES,
    LYRICS_TRANSLATION_OPTIONS,
  } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';
  import { immersiveState } from './immersive.svelte';
  import { NOW_PLAYING_STYLES } from './nowPlayingStyle';

  interface Props {
    showAmbientToggle?: boolean;
    ambientEnabled?: boolean;
    onToggleAmbient?: () => void;
    lyricsVisible?: boolean;
    onToggleLyrics?: () => void;
    focusMode?: boolean;
    onToggleFocus?: () => void;
  }

  let {
    showAmbientToggle = true,
    ambientEnabled = true,
    onToggleAmbient,
    lyricsVisible = true,
    onToggleLyrics,
    focusMode = false,
    onToggleFocus,
  }: Props = $props();

  type SubmenuKey = 'source' | 'romanisation' | 'translation' | 'textSize' | 'animation' | null;

  let open = $state(false);
  let activeSubmenu = $state<SubmenuKey>(null);

  let track = $derived(playerStore.currentTrack);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  let providerLabel = $derived(
    LYRICS_PROVIDERS.find((p) => p.id === featuresStore.lyricsProviderId)?.label ?? 'Auto'
  );
  let translationLabel = $derived(
    LYRICS_TRANSLATION_OPTIONS.find((t) => t.id === featuresStore.lyricsTranslationLang)?.label ?? 'Off'
  );
  let textScaleLabel = $derived(
    LYRICS_TEXT_SCALES.reduce((best, cur) =>
      Math.abs(cur.scale - featuresStore.lyricsFontScale) < Math.abs(best.scale - featuresStore.lyricsFontScale)
        ? cur
        : best
    ).label
  );
  let animationLabel = $derived(
    LYRICS_ANIMATION_OPTIONS.find((a) => a.id === featuresStore.lyricsAnimation)?.label ?? 'Apple Fluid'
  );

  function toggleMenu() {
    open = !open;
    if (!open) activeSubmenu = null;
    immersiveState.setMenuOpen(open);
  }

  function closeMenu() {
    open = false;
    activeSubmenu = null;
    immersiveState.setMenuOpen(false);
  }

  function formatOffset(ms: number): string {
    if (ms === 0) return '0.0s';
    const sign = ms > 0 ? '+' : '-';
    return `${sign}${(Math.abs(ms) / 1000).toFixed(1)}s`;
  }

  function selectStyle(styleId: (typeof NOW_PLAYING_STYLES)[number]['id']) {
    settingsStore.nowPlayingStyle = styleId;
    settingsStore.saveStringPref('lw_now_playing_style', styleId);
  }

  function changeProvider(providerId: string) {
    featuresStore.lyricsProviderId = providerId;
    if (track) {
      void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true, providerId });
    }
    activeSubmenu = null;
  }
</script>

<div class="flex items-center gap-1.5">
  {#if showAmbientToggle && onToggleAmbient}
    <button
      type="button"
      onclick={() => {
        immersiveState.ping();
        onToggleAmbient?.();
      }}
      aria-pressed={ambientEnabled}
      use:waveTooltip={ambientEnabled ? 'Disable ambient' : 'Enable ambient'}
      aria-label={ambientEnabled ? 'Disable ambient' : 'Enable ambient'}
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border transition {ambientEnabled
        ? 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-accent)]'
        : 'border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
    >
      <WaveIcon name="sparkles" size={15} />
    </button>
  {/if}

  <button
    type="button"
    onclick={() => immersiveState.toggleImmersive()}
    aria-pressed={immersiveState.isImmersive}
    use:waveTooltip={immersiveState.isImmersive ? 'Exit fullscreen (F11)' : 'Fullscreen (F11)'}
    aria-label={immersiveState.isImmersive ? 'Exit fullscreen' : 'Fullscreen'}
    class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border transition {immersiveState.isImmersive
      ? 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-accent)]'
      : 'border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
  >
    <WaveIcon name={immersiveState.isImmersive ? 'minimize' : 'maximize'} size={15} />
  </button>

  <div class="relative" use:dismissFlyout={{ open, close: closeMenu }}>
    <button
      type="button"
      onclick={toggleMenu}
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] transition hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
      use:waveTooltip={'More options'}
      aria-label="More options"
      aria-expanded={open}
      aria-controls="lw-now-playing-options"
    >
      <WaveIcon name="more" size={16} />
    </button>

    <div
      id="lw-now-playing-options"
      data-open={open}
      inert={!open}
      class="lw-popover now-playing-menu absolute right-0 top-10 z-40 w-[290px] rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 shadow-[var(--lw-shadow-flyout)]"
      class:immersive={immersiveState.isImmersive}
      class:has-submenu={activeSubmenu !== null}
    >
      {#if activeSubmenu !== null}
        <!-- Submenu View -->
        <div class="flex shrink-0 items-center gap-2 border-b border-[var(--lw-divider)] px-2 py-1.5">
          <button
            type="button"
            onclick={() => (activeSubmenu = null)}
            class="flex h-6 w-6 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
          >
            <WaveIcon name="chevronLeft" size={14} />
          </button>
          <span class="text-[12.5px] font-bold text-[var(--lw-text-primary)]">
            {activeSubmenu === 'source'
              ? 'Lyrics source'
              : activeSubmenu === 'romanisation'
                ? 'Romanisation'
                : activeSubmenu === 'translation'
                  ? 'Translation'
                  : activeSubmenu === 'textSize'
                    ? 'Text size'
                    : 'Animation style'}
          </span>
        </div>

        <div use:waveSmoothScroll class="lw-scroll min-h-0 max-h-[320px] space-y-0.5 overflow-y-auto p-1">
          {#if activeSubmenu === 'source'}
            {#each LYRICS_PROVIDERS as p (p.id)}
              <button
                type="button"
                onclick={() => changeProvider(p.id)}
                class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
              >
                <span>{p.label}</span>
                {#if featuresStore.lyricsProviderId === p.id}
                  <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
                {/if}
              </button>
            {/each}
          {:else if activeSubmenu === 'romanisation'}
            <button
              type="button"
              onclick={() => {
                featuresStore.setLyricsRomanized(false);
                activeSubmenu = null;
              }}
              class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
            >
              <span>Off</span>
              {#if !featuresStore.showRomanized}
                <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
              {/if}
            </button>
            <button
              type="button"
              onclick={() => {
                featuresStore.setLyricsRomanized(true);
                activeSubmenu = null;
              }}
              class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
            >
              <span>On</span>
              {#if featuresStore.showRomanized}
                <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
              {/if}
            </button>
          {:else if activeSubmenu === 'translation'}
            {#each LYRICS_TRANSLATION_OPTIONS as opt (opt.id)}
              <button
                type="button"
                onclick={() => {
                  featuresStore.setLyricsTranslationLang(opt.id);
                  activeSubmenu = null;
                }}
                class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
              >
                <span>{opt.label}</span>
                {#if featuresStore.lyricsTranslationLang === opt.id}
                  <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
                {/if}
              </button>
            {/each}
          {:else if activeSubmenu === 'textSize'}
            {#each LYRICS_TEXT_SCALES as item (item.scale)}
              <button
                type="button"
                onclick={() => {
                  featuresStore.setLyricsTextScale(item.scale);
                  activeSubmenu = null;
                }}
                class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
              >
                <span>{item.label}</span>
                {#if Math.abs(featuresStore.lyricsFontScale - item.scale) < 0.05}
                  <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
                {/if}
              </button>
            {/each}
          {:else if activeSubmenu === 'animation'}
            {#each LYRICS_ANIMATION_OPTIONS as anim (anim.id)}
              <button
                type="button"
                onclick={() => {
                  featuresStore.setLyricsAnimation(anim.id);
                  activeSubmenu = null;
                }}
                class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
              >
                <span>{anim.label}</span>
                {#if featuresStore.lyricsAnimation === anim.id}
                  <WaveIcon name="check" size={13} class="text-[var(--lw-accent)]" />
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      {:else}
        <!-- 1. Lyrics offset row -->
        {#if track}
          <div class="flex items-center justify-between gap-2 px-2.5 py-2">
            <div>
              <div class="text-[13px] font-semibold text-[var(--lw-text-primary)]">Lyrics offset</div>
              {#if offsetMs !== 0}
                <div class="text-[11px] font-medium text-[var(--lw-accent)]">{formatOffset(offsetMs)}</div>
              {/if}
            </div>
            <div class="flex items-center gap-0.5 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-1 py-0.5 lw-tabular">
              <button
                type="button"
                onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
                class="flex h-6 w-6 cursor-pointer items-center justify-center rounded text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
                use:waveTooltip={'Delay -0.5s'}
              >−</button>
              <span class="min-w-[42px] text-center text-[12px] font-semibold {offsetMs !== 0 ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]'}">
                {formatOffset(offsetMs)}
              </span>
              <button
                type="button"
                onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
                class="flex h-6 w-6 cursor-pointer items-center justify-center rounded text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
                use:waveTooltip={'Advance +0.5s'}
              >+</button>
              {#if offsetMs !== 0}
                <button
                  type="button"
                  onclick={() => featuresStore.resetLyricsOffset(track.id)}
                  class="flex h-6 w-6 cursor-pointer items-center justify-center rounded text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
                  use:waveTooltip={'Reset to 0.0s'}
                >
                  <WaveIcon name="close" size={11} />
                </button>
              {/if}
            </div>
          </div>
          <div class="my-1 h-px bg-[var(--lw-divider)]"></div>
        {/if}

        <!-- 2. Submenu rows matching OG screenshot -->
        <button
          type="button"
          onclick={() => (activeSubmenu = 'source')}
          class="menu-row"
        >
          <WaveIcon name="lyrics" size={14} class="shrink-0 text-[var(--lw-text-secondary)]" />
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Lyrics source</span>
          <span class="text-[12px] {featuresStore.lyricsProviderId !== 'auto' ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{providerLabel}</span>
          <WaveIcon name="chevronRight" size={13} class="text-[var(--lw-text-tertiary)]" />
        </button>

        <button
          type="button"
          onclick={() => (activeSubmenu = 'romanisation')}
          class="menu-row"
        >
          <WaveIcon name="bookmark" size={14} class="shrink-0 {featuresStore.showRomanized ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Romanisation</span>
          <span class="text-[12px] {featuresStore.showRomanized ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{featuresStore.showRomanized ? 'On' : 'Off'}</span>
          <WaveIcon name="chevronRight" size={13} class="text-[var(--lw-text-tertiary)]" />
        </button>

        <button
          type="button"
          onclick={() => (activeSubmenu = 'translation')}
          class="menu-row"
        >
          <WaveIcon name="globe" size={14} class="shrink-0 {featuresStore.lyricsTranslationLang !== 'off' ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Translation</span>
          <span class="text-[12px] {featuresStore.lyricsTranslationLang !== 'off' ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{translationLabel}</span>
          <WaveIcon name="chevronRight" size={13} class="text-[var(--lw-text-tertiary)]" />
        </button>

        <button
          type="button"
          onclick={() => (activeSubmenu = 'textSize')}
          class="menu-row"
        >
          <span class="w-[14px] shrink-0 text-center text-[11px] font-bold {Math.abs(featuresStore.lyricsFontScale - 1.0) > 0.05 ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}">AA</span>
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Text size</span>
          <span class="text-[12px] {Math.abs(featuresStore.lyricsFontScale - 1.0) > 0.05 ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{textScaleLabel}</span>
          <WaveIcon name="chevronRight" size={13} class="text-[var(--lw-text-tertiary)]" />
        </button>

        <button
          type="button"
          onclick={() => (activeSubmenu = 'animation')}
          class="menu-row"
        >
          <WaveIcon name="wand" size={14} class="shrink-0 text-[var(--lw-text-secondary)]" />
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Animation style</span>
          <span class="text-[12px] text-[var(--lw-text-tertiary)]">{animationLabel}</span>
          <WaveIcon name="chevronRight" size={13} class="text-[var(--lw-text-tertiary)]" />
        </button>

        <div class="my-1 h-px bg-[var(--lw-divider)]"></div>

        <!-- 3. View & Mode Toggles -->
        {#if onToggleLyrics}
          <button
            type="button"
            onclick={() => {
              onToggleLyrics?.();
            }}
            class="menu-row"
          >
            <WaveIcon name="lyrics" size={14} class="shrink-0 {lyricsVisible ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
            <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Lyrics</span>
            <span class="text-[12px] {lyricsVisible ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{lyricsVisible ? 'Visible' : 'Hidden'}</span>
          </button>
        {/if}

        {#if onToggleFocus}
          <button
            type="button"
            onclick={() => {
              onToggleFocus?.();
            }}
            class="menu-row"
          >
            <WaveIcon name={focusMode ? 'minimize' : 'maximize'} size={14} class="shrink-0 {focusMode ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
            <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Focus on lyrics</span>
            <span class="text-[12px] {focusMode ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{focusMode ? 'On' : 'Off'}</span>
          </button>
        {/if}

        {#if showAmbientToggle && onToggleAmbient}
          <button
            type="button"
            onclick={() => {
              onToggleAmbient?.();
            }}
            class="menu-row"
          >
            <WaveIcon name="sparkles" size={14} class="shrink-0 {ambientEnabled ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
            <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Ambient</span>
            <span class="text-[12px] {ambientEnabled ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{ambientEnabled ? 'On' : 'Off'}</span>
          </button>
        {/if}

        <button
          type="button"
          onclick={() => immersiveState.toggleImmersive()}
          class="menu-row"
        >
          <WaveIcon name={immersiveState.isImmersive ? 'minimize' : 'maximize'} size={14} class="shrink-0 {immersiveState.isImmersive ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}" />
          <span class="flex-1 text-left font-medium text-[var(--lw-text-primary)]">Fullscreen</span>
          <span class="text-[12px] text-[var(--lw-text-tertiary)]">F11</span>
        </button>

        <div class="my-1 h-px bg-[var(--lw-divider)]"></div>

        <!-- 4. Inline NOW PLAYING STYLE segmented control -->
        <div class="px-2 pt-1 pb-1.5">
          <div class="mb-1.5 text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
            NOW PLAYING STYLE
          </div>
          <div class="grid grid-cols-3 gap-0.5 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-[3px]">
            {#each NOW_PLAYING_STYLES as style (style.id)}
              {@const selected = settingsStore.nowPlayingStyle === style.id}
              <button
                type="button"
                onclick={() => selectStyle(style.id)}
                class="cursor-pointer rounded-[6px] py-1.5 text-center text-[11.5px] transition {selected
                  ? 'border border-[var(--lw-accent)] bg-[var(--lw-surface-raised)] font-semibold text-[var(--lw-text-primary)]'
                  : 'border border-transparent font-medium text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
              >
                {style.label}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .now-playing-menu {
    /* Leave room for the toolbar, its popover offset, and a gap below the menu. */
    max-height: calc(100dvh - var(--lw-titlebar-h) - var(--lw-dock-h) - 70px);
  }
  .now-playing-menu.immersive {
    max-height: calc(100dvh - 70px);
  }
  .now-playing-menu.has-submenu[data-open='true'] {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .menu-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 12.5px;
    cursor: pointer;
    transition: background-color var(--lw-motion-fast);
  }
  .menu-row:hover {
    background: var(--lw-wash-hover);
  }
</style>
