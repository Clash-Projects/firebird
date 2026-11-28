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
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let closeSubTimer: ReturnType<typeof setTimeout> | null = null;

  let track = $derived(playerStore.currentTrack);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);

  let resolvedSource = $derived(
    track ? featuresStore.lyricsByTrack[track.id]?.provider || '' : ''
  );
  let providerLabel = $derived(
    resolvedSource ||
      (LYRICS_PROVIDERS.find((p) => p.id === featuresStore.lyricsProviderId)?.label ?? 'Auto')
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

  function clearTimers() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    if (closeSubTimer) {
      clearTimeout(closeSubTimer);
      closeSubTimer = null;
    }
  }

  function hoverSubmenu(key: Exclude<SubmenuKey, null>) {
    if (closeSubTimer) {
      clearTimeout(closeSubTimer);
      closeSubTimer = null;
    }
    if (activeSubmenu === key) return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      activeSubmenu = key;
    }, activeSubmenu !== null ? 300 : 180);
  }

  function openSubmenuNow(key: Exclude<SubmenuKey, null>) {
    clearTimers();
    activeSubmenu = activeSubmenu === key ? null : key;
  }

  function scheduleCloseSubmenu() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    if (!activeSubmenu) return;
    if (closeSubTimer) clearTimeout(closeSubTimer);
    closeSubTimer = setTimeout(() => {
      activeSubmenu = null;
    }, 340);
  }

  function toggleMenu() {
    open = !open;
    if (!open) {
      clearTimers();
      activeSubmenu = null;
    }
    immersiveState.setMenuOpen(open);
  }

  function closeMenu() {
    clearTimers();
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

  function changeProvider(providerId: string | null) {
    if (providerId === null) {
      const idx = LYRICS_PROVIDERS.findIndex((p) => p.id === featuresStore.lyricsProviderId);
      const next = LYRICS_PROVIDERS[(idx + 1) % LYRICS_PROVIDERS.length];
      featuresStore.lyricsProviderId = next.id;
      if (track) {
        void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true, providerId: next.id });
      }
      closeMenu();
      return;
    }
    featuresStore.lyricsProviderId = providerId;
    settingsStore.saveStringPref('lw_lyrics_provider', providerId);
    if (track) {
      void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true, providerId });
    }
    closeMenu();
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
      class="np-quick-btn"
      data-active={ambientEnabled}
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
    class="np-quick-btn mr-0.5"
    data-active={immersiveState.isImmersive}
  >
    <WaveIcon name={immersiveState.isImmersive ? 'minimize' : 'maximize'} size={15} />
  </button>

  <div class="relative" use:dismissFlyout={{ open, close: closeMenu }}>
    <button
      type="button"
      onclick={toggleMenu}
      class="np-more-btn"
      use:waveTooltip={'More options'}
      aria-label="More options"
      aria-expanded={open}
      aria-controls="lw-now-playing-options"
    >
      <WaveIcon name="more" size={15} />
    </button>

    <div
      id="lw-now-playing-options"
      data-open={open}
      inert={!open}
      class="lw-popover lw-has-cascade now-playing-flyout absolute top-10 right-0 z-40 w-[300px]"
    >
      <!-- 1. Timing Offset Row (stays open while tapping) -->
      {#if track}
        <div
          role="presentation"
          onpointerenter={scheduleCloseSubmenu}
          class="flex items-center justify-between gap-2 px-3 pt-2 pb-1.5"
        >
          <div>
            <div class="text-[13px] font-semibold text-[var(--lw-text-primary)]">Lyrics offset</div>
            {#if offsetMs !== 0}
              <div class="text-[11px] font-medium text-[var(--lw-accent)] lw-tabular">{formatOffset(offsetMs)}</div>
            {/if}
          </div>
          <div class="flex items-center gap-0.5 rounded-[10px] border border-white/12 bg-white/8 p-0.5 lw-tabular">
            <button
              type="button"
              onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
              class="flex h-6 w-6 cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
              use:waveTooltip={'-0.5s'}
            >−</button>
            <span class="min-w-[42px] text-center text-[12px] font-semibold {offsetMs !== 0 ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-primary)]'}">
              {formatOffset(offsetMs)}
            </span>
            <button
              type="button"
              onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
              class="flex h-6 w-6 cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
              use:waveTooltip={'+0.5s'}
            >+</button>
            {#if offsetMs !== 0}
              <button
                type="button"
                onclick={() => featuresStore.resetLyricsOffset(track.id)}
                class="flex h-6 w-6 cursor-pointer items-center justify-center rounded-[7px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]"
                use:waveTooltip={'Reset to 0.0s'}
              >
                <WaveIcon name="close" size={11} />
              </button>
            {/if}
          </div>
        </div>
        <div class="lw-menu-sep"></div>
      {/if}

      <!-- 2. Cascading Second-Panel Submenus (matches _CascadingSubmenuRow in now_playing_overflow_menu.dart) -->
      <!-- Lyrics source -->
      <div
        role="presentation"
        class="relative"
        onpointerenter={() => hoverSubmenu('source')}
        onpointerleave={scheduleCloseSubmenu}
      >
        <button
          type="button"
          data-highlighted={activeSubmenu === 'source'}
          onclick={() => openSubmenuNow('source')}
          class="lw-menu-row"
        >
          <WaveIcon name="lyrics" size={15} class={featuresStore.lyricsProviderId !== 'auto' ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Lyrics source</span>
          <span class="text-[12px] {featuresStore.lyricsProviderId !== 'auto' ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{providerLabel}</span>
          <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
        </button>

        {#if activeSubmenu === 'source'}
          <div
            role="menu"
            tabindex="-1"
            onpointerenter={clearTimers}
            onpointerleave={scheduleCloseSubmenu}
            class="absolute top-[-4px] right-full z-50 w-[240px] pr-1.5"
          >
            <div class="now-playing-flyout max-h-[360px] overflow-y-auto">
              {#each LYRICS_PROVIDERS as p (p.id)}
                {@const active = featuresStore.lyricsProviderId === p.id}
                <button
                  type="button"
                  onclick={() => changeProvider(p.id)}
                  class="lw-menu-row"
                >
                  <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if active}<WaveIcon name="check" size={12} />{/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">{p.label}</span>
                </button>
              {/each}
              <div class="lw-menu-sep"></div>
              <button
                type="button"
                onclick={() => changeProvider(null)}
                class="lw-menu-row"
              >
                <WaveIcon name="refresh" size={13} />
                <span class="min-w-0 flex-1 truncate">Try another source</span>
              </button>
            </div>
          </div>
        {/if}
      </div>

      <!-- Romanisation -->
      <div
        role="presentation"
        class="relative"
        onpointerenter={() => hoverSubmenu('romanisation')}
        onpointerleave={scheduleCloseSubmenu}
      >
        <button
          type="button"
          data-highlighted={activeSubmenu === 'romanisation'}
          onclick={() => openSubmenuNow('romanisation')}
          class="lw-menu-row"
        >
          <WaveIcon name="bookmark" size={15} class={featuresStore.showRomanized ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Romanisation</span>
          <span class="text-[12px] {featuresStore.showRomanized ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{featuresStore.showRomanized ? 'On' : 'Off'}</span>
          <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
        </button>

        {#if activeSubmenu === 'romanisation'}
          <div
            role="menu"
            tabindex="-1"
            onpointerenter={clearTimers}
            onpointerleave={scheduleCloseSubmenu}
            class="absolute top-[-4px] right-full z-50 w-[184px] pr-1.5"
          >
            <div class="now-playing-flyout">
              <button
                type="button"
                onclick={() => {
                  featuresStore.setLyricsRomanized(false);
                  closeMenu();
                }}
                class="lw-menu-row"
              >
                <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                  {#if !featuresStore.showRomanized}<WaveIcon name="check" size={12} />{/if}
                </span>
                <span class="min-w-0 flex-1 truncate">Off</span>
              </button>
              <button
                type="button"
                onclick={() => {
                  featuresStore.setLyricsRomanized(true);
                  closeMenu();
                }}
                class="lw-menu-row"
              >
                <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                  {#if featuresStore.showRomanized}<WaveIcon name="check" size={12} />{/if}
                </span>
                <span class="min-w-0 flex-1 truncate">On</span>
              </button>
            </div>
          </div>
        {/if}
      </div>

      <!-- Translation -->
      <div
        role="presentation"
        class="relative"
        onpointerenter={() => hoverSubmenu('translation')}
        onpointerleave={scheduleCloseSubmenu}
      >
        <button
          type="button"
          data-highlighted={activeSubmenu === 'translation'}
          onclick={() => openSubmenuNow('translation')}
          class="lw-menu-row"
        >
          <WaveIcon name="globe" size={15} class={featuresStore.lyricsTranslationLang !== 'off' ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Translation</span>
          <span class="text-[12px] {featuresStore.lyricsTranslationLang !== 'off' ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{translationLabel}</span>
          <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
        </button>

        {#if activeSubmenu === 'translation'}
          <div
            role="menu"
            tabindex="-1"
            onpointerenter={clearTimers}
            onpointerleave={scheduleCloseSubmenu}
            class="absolute top-[-4px] right-full z-50 w-[208px] pr-1.5"
          >
            <div class="now-playing-flyout max-h-[340px] overflow-y-auto">
              {#each LYRICS_TRANSLATION_OPTIONS as opt (opt.id)}
                <button
                  type="button"
                  onclick={() => {
                    featuresStore.setLyricsTranslationLang(opt.id);
                    closeMenu();
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if featuresStore.lyricsTranslationLang === opt.id}<WaveIcon name="check" size={12} />{/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">{opt.label}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Text size -->
      <div
        role="presentation"
        class="relative"
        onpointerenter={() => hoverSubmenu('textSize')}
        onpointerleave={scheduleCloseSubmenu}
      >
        <button
          type="button"
          data-highlighted={activeSubmenu === 'textSize'}
          onclick={() => openSubmenuNow('textSize')}
          class="lw-menu-row"
        >
          <span class="w-[15px] shrink-0 text-center text-[11px] font-bold {Math.abs(featuresStore.lyricsFontScale - 1.0) > 0.05 ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'}">AA</span>
          <span class="min-w-0 flex-1 truncate font-medium">Text size</span>
          <span class="text-[12px] {Math.abs(featuresStore.lyricsFontScale - 1.0) > 0.05 ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{textScaleLabel}</span>
          <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
        </button>

        {#if activeSubmenu === 'textSize'}
          <div
            role="menu"
            tabindex="-1"
            onpointerenter={clearTimers}
            onpointerleave={scheduleCloseSubmenu}
            class="absolute top-[-4px] right-full z-50 w-[196px] pr-1.5"
          >
            <div class="now-playing-flyout">
              {#each LYRICS_TEXT_SCALES as item (item.scale)}
                <button
                  type="button"
                  onclick={() => {
                    featuresStore.setLyricsTextScale(item.scale);
                    closeMenu();
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if Math.abs(featuresStore.lyricsFontScale - item.scale) < 0.05}<WaveIcon name="check" size={12} />{/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">{item.label}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Animation style -->
      <div
        role="presentation"
        class="relative"
        onpointerenter={() => hoverSubmenu('animation')}
        onpointerleave={scheduleCloseSubmenu}
      >
        <button
          type="button"
          data-highlighted={activeSubmenu === 'animation'}
          onclick={() => openSubmenuNow('animation')}
          class="lw-menu-row"
        >
          <WaveIcon name="palette" size={15} class="text-[var(--lw-text-secondary)]" />
          <span class="min-w-0 flex-1 truncate font-medium">Animation style</span>
          <span class="text-[12px] text-[var(--lw-text-tertiary)]">{animationLabel}</span>
          <WaveIcon name="chevronRight" size={12} class="text-[var(--lw-text-tertiary)]" />
        </button>

        {#if activeSubmenu === 'animation'}
          <div
            role="menu"
            tabindex="-1"
            onpointerenter={clearTimers}
            onpointerleave={scheduleCloseSubmenu}
            class="absolute bottom-[-4px] right-full z-50 w-[216px] pr-1.5"
          >
            <div class="now-playing-flyout max-h-[340px] overflow-y-auto">
              {#each LYRICS_ANIMATION_OPTIONS as anim (anim.id)}
                <button
                  type="button"
                  onclick={() => {
                    featuresStore.setLyricsAnimation(anim.id);
                    closeMenu();
                  }}
                  class="lw-menu-row"
                >
                  <span class="flex w-[13px] shrink-0 items-center justify-center text-[var(--lw-accent)]">
                    {#if featuresStore.lyricsAnimation === anim.id}<WaveIcon name="check" size={12} />{/if}
                  </span>
                  <span class="min-w-0 flex-1 truncate">{anim.label}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div class="lw-menu-sep"></div>

      <!-- 3. View & Mode Toggles -->
      {#if onToggleLyrics}
        <button
          type="button"
          onpointerenter={scheduleCloseSubmenu}
          onclick={() => onToggleLyrics?.()}
          class="lw-menu-row"
        >
          <WaveIcon name="lyrics" size={15} class={lyricsVisible ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Lyrics</span>
          <span class="text-[12px] {lyricsVisible ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{lyricsVisible ? 'Visible' : 'Hidden'}</span>
        </button>
      {/if}

      {#if onToggleFocus}
        <button
          type="button"
          onpointerenter={scheduleCloseSubmenu}
          onclick={() => onToggleFocus?.()}
          class="lw-menu-row"
        >
          <WaveIcon name={focusMode ? 'minimize' : 'maximize'} size={15} class={focusMode ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Focus on lyrics</span>
          <span class="text-[12px] {focusMode ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{focusMode ? 'On' : 'Off'}</span>
        </button>
      {/if}

      {#if showAmbientToggle && onToggleAmbient}
        <button
          type="button"
          onpointerenter={scheduleCloseSubmenu}
          onclick={() => onToggleAmbient?.()}
          class="lw-menu-row"
        >
          <WaveIcon name="sparkles" size={15} class={ambientEnabled ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
          <span class="min-w-0 flex-1 truncate font-medium">Ambient</span>
          <span class="text-[12px] {ambientEnabled ? 'font-semibold text-[var(--lw-accent)]' : 'text-[var(--lw-text-tertiary)]'}">{ambientEnabled ? 'On' : 'Off'}</span>
        </button>
      {/if}

      <button
        type="button"
        onpointerenter={scheduleCloseSubmenu}
        onclick={() => immersiveState.toggleImmersive()}
        class="lw-menu-row"
      >
        <WaveIcon name={immersiveState.isImmersive ? 'minimize' : 'maximize'} size={15} class={immersiveState.isImmersive ? 'text-[var(--lw-accent)]' : 'text-[var(--lw-text-secondary)]'} />
        <span class="min-w-0 flex-1 truncate font-medium">Fullscreen</span>
        <span class="lw-menu-hint">F11</span>
      </button>

      <div class="lw-menu-sep"></div>

      <!-- 4. Inline NOW PLAYING STYLE segmented control -->
      <div
        role="presentation"
        onpointerenter={scheduleCloseSubmenu}
        class="px-2.5 pt-1 pb-2"
      >
        <div class="mb-1.5 text-[10px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">
          NOW PLAYING STYLE
        </div>
        <div class="grid grid-cols-3 gap-0.5 rounded-[8px] border border-white/10 bg-white/8 p-[3px]">
          {#each NOW_PLAYING_STYLES as style (style.id)}
            {@const selected = settingsStore.nowPlayingStyle === style.id}
            <button
              type="button"
              onclick={() => selectStyle(style.id)}
              class="cursor-pointer rounded-[6px] py-1.5 text-center text-[11.5px] transition {selected
                ? 'border border-[var(--lw-accent)]/50 bg-white/16 font-semibold text-[var(--lw-text-primary)]'
                : 'border border-transparent font-normal text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
            >
              {style.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  /* Matches _QuickActionButton & _CircularIconButton in now_playing_overflow_menu.dart */
  .np-quick-btn {
    display: flex;
    width: 32px;
    height: 32px;
    align-items: center;
    justify-content: center;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.06);
    color: var(--lw-text-secondary);
    cursor: pointer;
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    transition:
      background-color 110ms var(--ease-native),
      border-color 110ms var(--ease-native),
      color 110ms var(--ease-native);
  }

  .np-quick-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: var(--lw-text-primary);
  }

  .np-quick-btn[data-active="true"] {
    border-color: color-mix(in srgb, var(--lw-accent) 45%, transparent);
    background: color-mix(in srgb, var(--lw-accent) 16%, transparent);
    color: var(--lw-accent);
  }

  .np-more-btn {
    display: flex;
    width: 32px;
    height: 32px;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    background: rgba(255, 255, 255, 0.08);
    color: var(--lw-text-secondary);
    cursor: pointer;
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    transition:
      background-color 110ms var(--ease-native),
      color 110ms var(--ease-native);
  }

  .np-more-btn:hover {
    background: rgba(255, 255, 255, 0.16);
    color: var(--lw-text-primary);
  }

  .now-playing-flyout {
    box-sizing: border-box;
    border-radius: 14px;
    padding: 4px;
    background: rgba(20, 20, 24, 0.72);
    border: 1px solid rgba(255, 255, 255, 0.14);
    box-shadow: var(--lw-shadow-flyout);
    color: var(--lw-text-primary);
    backdrop-filter: blur(30px) saturate(1.2);
    -webkit-backdrop-filter: blur(30px) saturate(1.2);
    scrollbar-width: none;
  }

  .now-playing-flyout::-webkit-scrollbar {
    display: none;
  }

  :global(:root[data-theme="light"]) .now-playing-flyout {
    background: rgba(245, 245, 247, 0.84);
    border-color: rgba(0, 0, 0, 0.10);
  }
</style>
