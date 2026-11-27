<script lang="ts">
  import { dismissFlyout } from '$src/app/accessibility';
  import { featuresStore, LYRICS_PROVIDERS } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Select from '$src/ui/components/Select.svelte';
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

  let open = $state(false);
  let track = $derived(playerStore.currentTrack);
  let offsetMs = $derived(track ? featuresStore.getLyricsOffset(track.id) : 0);
  const providerOptions = LYRICS_PROVIDERS.map(({ id, label }) => ({ value: id, label }));

  function toggleMenu() {
    open = !open;
    immersiveState.setMenuOpen(open);
  }

  function closeMenu() {
    open = false;
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
    closeMenu();
  }

  function changeProvider(providerId: string) {
    featuresStore.lyricsProviderId = providerId;
    if (track) {
      void featuresStore.fetchLyricsForTrack(track, { forceRefresh: true, providerId });
    }
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
      title={ambientEnabled ? 'Disable ambient' : 'Enable ambient'}
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border transition {ambientEnabled
        ? 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-accent)]'
        : 'border-transparent bg-white/10 text-[var(--lw-text-secondary)] hover:bg-white/20 hover:text-[var(--lw-text-primary)]'}"
    >
      <WaveIcon name="sparkles" size={15} />
    </button>
  {/if}

  <button
    type="button"
    onclick={() => immersiveState.toggleImmersive()}
    aria-pressed={immersiveState.isImmersive}
    title={immersiveState.isImmersive ? 'Exit fullscreen (F11)' : 'Fullscreen (F11)'}
    class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] border transition {immersiveState.isImmersive
      ? 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-accent)]'
      : 'border-transparent bg-white/10 text-[var(--lw-text-secondary)] hover:bg-white/20 hover:text-[var(--lw-text-primary)]'}"
  >
    <WaveIcon name={immersiveState.isImmersive ? 'minimize' : 'maximize'} size={15} />
  </button>

  <div class="relative" use:dismissFlyout={{ open, close: closeMenu }}>
    <button
      type="button"
      onclick={toggleMenu}
      class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-[6px] bg-white/10 text-[var(--lw-text-secondary)] transition hover:bg-white/20 hover:text-[var(--lw-text-primary)]"
      title="More options"
      aria-expanded={open}
      aria-controls="lw-now-playing-options"
    >
      <WaveIcon name="more" size={16} />
    </button>

    <div
      id="lw-now-playing-options"
      data-open={open}
      inert={!open}
      class="lw-popover absolute right-0 top-10 z-40 w-[264px] rounded-[14px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-2.5 shadow-[var(--lw-shadow-flyout)]"
    >
      <div class="px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
        Style
      </div>
      {#each NOW_PLAYING_STYLES as style (style.id)}
        <button
          type="button"
          onclick={() => selectStyle(style.id)}
          class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
        >
          <span>{style.label}</span>
          {#if settingsStore.nowPlayingStyle === style.id}
            <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
          {/if}
        </button>
      {/each}

      {#if onToggleLyrics || onToggleFocus}
        <div class="my-1.5 h-px bg-[var(--lw-divider)]"></div>
        {#if onToggleLyrics}
          <button
            type="button"
            onclick={() => {
              onToggleLyrics?.();
              closeMenu();
            }}
            class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
          >
            <span class="flex items-center gap-2"><WaveIcon name="lyrics" size={14} /> Lyrics</span>
            {#if lyricsVisible}
              <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
            {/if}
          </button>
        {/if}
        {#if onToggleFocus && lyricsVisible}
          <button
            type="button"
            onclick={() => {
              onToggleFocus?.();
              closeMenu();
            }}
            class="flex w-full cursor-pointer items-center justify-between rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
          >
            <span>Focus lyrics</span>
            {#if focusMode}
              <WaveIcon name="check" size={14} class="text-[var(--lw-accent)]" />
            {/if}
          </button>
        {/if}
      {/if}

      {#if track}
        <div class="my-1.5 h-px bg-[var(--lw-divider)]"></div>
        <div class="px-2 py-1 text-[10.5px] font-bold tracking-[1px] text-[var(--lw-text-tertiary)] uppercase">
          Lyrics
        </div>
        <div class="space-y-1.5 px-2 py-1">
          <div class="flex items-center justify-between gap-2 text-[12px]">
            <span class="text-[var(--lw-text-secondary)]">Timing</span>
            <div class="flex items-center gap-1 rounded-[8px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] px-1 py-0.5 text-[11px] lw-tabular">
              <button
                type="button"
                onclick={() => featuresStore.adjustLyricsOffset(track.id, -500)}
                class="h-5 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
                title="Delay -0.5s"
              >−</button>
              <button
                type="button"
                onclick={() => featuresStore.resetLyricsOffset(track.id)}
                class="px-1 text-[var(--lw-text-primary)]"
                title="Reset timing offset"
              >{formatOffset(offsetMs)}</button>
              <button
                type="button"
                onclick={() => featuresStore.adjustLyricsOffset(track.id, 500)}
                class="h-5 w-5 cursor-pointer text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]"
                title="Advance +0.5s"
              >+</button>
            </div>
          </div>

          <div class="flex gap-1.5">
            <button
              type="button"
              onclick={() => (featuresStore.showRomanized = !featuresStore.showRomanized)}
              class="flex-1 cursor-pointer rounded-[8px] border py-1 text-[11.5px] font-semibold transition {featuresStore.showRomanized
                ? 'border-[var(--lw-accent)] bg-[var(--lw-accent-subtle)] text-[var(--lw-accent)]'
                : 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
            >
              Romaji
            </button>
            <button
              type="button"
              onclick={() => (featuresStore.showTranslation = !featuresStore.showTranslation)}
              class="flex-1 cursor-pointer rounded-[8px] border py-1 text-[11.5px] font-semibold transition {featuresStore.showTranslation
                ? 'border-[var(--lw-accent)] bg-[var(--lw-accent-subtle)] text-[var(--lw-accent)]'
                : 'border-[var(--lw-outline)] bg-[var(--lw-surface)] text-[var(--lw-text-secondary)] hover:text-[var(--lw-text-primary)]'}"
            >
              Translate
            </button>
          </div>

          <Select
            label="Lyrics provider"
            value={featuresStore.lyricsProviderId}
            options={providerOptions}
            onchange={changeProvider}
            class="w-full"
          />
        </div>
      {/if}

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
    </div>
  </div>
</div>
