<script lang="ts">
  import { getPlatformAudioCopy } from '$src/features/audio_output/platformAudio';
  import { APP_FONTS } from '$src/features/settings/appFonts.svelte';
  import { setCustomBase, setThemeMode, syncThemeDocument } from '$src/features/settings/themeController.svelte';
  import { setUiScale, UI_SCALE_OPTIONS } from '$src/features/settings/uiScale.svelte';
  import NowPlayingStylePicker from '$src/features/settings/widgets/NowPlayingStylePicker.svelte';
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore, LYRICS_PROVIDERS } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import LastfmKeyGuide from '$src/ui/auth/LastfmKeyGuide.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import { CUSTOM_THEME_PRESETS } from '$src/ui/theme/tokens';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import BackupSettings from './BackupSettings.svelte';
  import CacheSettings from './CacheSettings.svelte';
  import DownloadLayoutSettings from './DownloadLayoutSettings.svelte';
  import LibrarySettings from './LibrarySettings.svelte';
  import PerformanceModeCard from './PerformanceModeCard.svelte';
  import PluginsSettings from './PluginsSettings.svelte';
  import ScrobblerSettings from './ScrobblerSettings.svelte';
  import { searchSettings } from './settingsSearch';
  import SpotifyConnect from './SpotifyConnect.svelte';
  import YtProfileChooser from './YtProfileChooser.svelte';

  interface Props {
    initialSection?: string;
  }

  let { initialSection = 'general' }: Props = $props();

  const sections: ReadonlyArray<{ id: string; label: string; icon: WaveIconName }> = [
    { id: 'general', label: 'General', icon: 'settings' },
    { id: 'integrations', label: 'Integrations', icon: 'globe' },
    { id: 'playback', label: 'Playback', icon: 'play' },
    { id: 'audio', label: 'Audio', icon: 'speaker' },
    { id: 'downloads', label: 'Downloads', icon: 'download' },
    { id: 'library', label: 'Library', icon: 'library' },
    { id: 'lyrics', label: 'Lyrics', icon: 'lyrics' },
    { id: 'appearance', label: 'Appearance', icon: 'palette' },
    { id: 'plugins', label: 'Plugins & Themes', icon: 'plugin' },
    { id: 'lastfm', label: 'Scrobbler', icon: 'heart' },
    { id: 'sources', label: 'Sources', icon: 'radio' },
    { id: 'experimental', label: 'Experimental', icon: 'wand' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

  let activeSection = $state('general');
  let query = $state('');
  let keyGuideOpen = $state(false);

  $effect(() => {
    if (initialSection && sections.some((s) => s.id === initialSection)) {
      activeSection = initialSection;
    }
  });

  let searchHits = $derived(searchSettings(query, settingsStore.platform));
  let audioCopy = $derived(getPlatformAudioCopy(settingsStore.platform));

  function toggleBitPerfect(enabled: boolean) {
    settingsStore.bitPerfectEnabled = enabled;
    settingsStore.exclusiveMode = enabled;
    void ipcInvoke('audio_output_set_bit_perfect', { enabled });
    void ipcInvoke('audio_output_set_exclusive', { enabled });
  }
</script>

<div data-lw="settings-page" class="flex h-full flex-col">
  <div class="px-6 pt-3.5">
    <div class="max-w-[640px]">
      <Input bind:value={query} placeholder="Search settings..." />
      {#if searchHits.length > 0}
        <div class="mt-1.5 space-y-0.5 rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface-raised)] p-1.5 shadow-[var(--lw-shadow-flyout)]">
          {#each searchHits as hit (hit.section)}
            <button
              type="button"
              onclick={() => {
                activeSection = hit.section;
                query = '';
              }}
              class="block w-full cursor-pointer rounded-[8px] px-2.5 py-1.5 text-left text-[12.5px] hover:bg-[var(--lw-wash-hover)]"
            >
              {hit.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="flex min-h-0 flex-1 gap-6 px-6 pt-4">
    <aside class="flex w-[220px] shrink-0 flex-col">
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-accent)] uppercase">SYSTEM</div>
      <h1 class="mb-3 text-[22px] font-bold tracking-[-0.4px]">Settings</h1>

      <nav class="lw-scroll flex-1 space-y-0.5 pb-6">
        {#each sections as sec (sec.id)}
          {@const active = activeSection === sec.id}
          <button
            type="button"
            onclick={() => (activeSection = sec.id)}
            class="flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-3 text-[13px] transition-colors {active
              ? 'bg-[var(--lw-wash-pressed)] font-semibold text-[var(--lw-text-primary)]'
              : 'text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
          >
            <WaveIcon name={sec.icon} size={16} />
            <span class="truncate">{sec.label}</span>
          </button>
        {/each}
      </nav>
    </aside>

    <div class="lw-scroll max-w-[780px] flex-1 space-y-5 pb-12 pr-2">
      <h2 class="text-[20px] font-bold tracking-[-0.3px]">
        {sections.find((s) => s.id === activeSection)?.label ?? 'Settings'}
      </h2>

    {#if activeSection === 'general'}
      <div class="space-y-3">
        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Language</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Interface display language</div>
          </div>
          <Select
            bind:value={settingsStore.language}
            options={[
              { value: 'en', label: 'English' },
              { value: 'es', label: 'Español' },
              { value: 'ja', label: '日本語' },
            ]}
          />
        </div>

        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">UI Scale</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Adjust desktop interface density</div>
          </div>
          <Select
            value={String(settingsStore.uiScale)}
            options={UI_SCALE_OPTIONS.map((v) => ({ value: String(v), label: `${Math.round(v * 100)}%` }))}
            onchange={(v) => setUiScale(Number(v))}
          />
        </div>

        <PerformanceModeCard />
      </div>
    {:else if activeSection === 'integrations'}
      <div class="space-y-3">
        <YtProfileChooser />
        <SpotifyConnect />
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Discord Rich Presence</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Show active track on your Discord profile</div>
          </div>
          <input type="checkbox" bind:checked={settingsStore.discordRpcEnabled} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">{audioCopy.mediaControlsTitle}</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Expose transport controls and timeline to the OS</div>
          </div>
          <input type="checkbox" bind:checked={settingsStore.smtcEnabled} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
      </div>
    {:else if activeSection === 'playback'}
      <div class="space-y-3">
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Gapless Playback</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Pre-warm standby deck for seamless transitions</div>
          </div>
          <input type="checkbox" bind:checked={settingsStore.gaplessPlayback} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Beat-Aware AutoMix</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Phase-lock beat grids during transitions</div>
          </div>
          <input type="checkbox" bind:checked={playerStore.automixEnabled} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
        <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div class="flex justify-between text-[13px]">
            <span class="font-semibold">Perceptual Crossfade</span>
            <span class="lw-tabular">{playerStore.crossfadeSec}s</span>
          </div>
          <input type="range" min="0" max="12" step="1" bind:value={playerStore.crossfadeSec} class="mt-2 h-1.5 w-full cursor-pointer accent-[var(--lw-accent)]" />
        </div>
      </div>
    {:else if activeSection === 'audio'}
      <div class="space-y-3">
        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Preferred Quality Waterfall</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Stepping down cleanly when unavailable</div>
          </div>
          <Select
            bind:value={settingsStore.qualityTier}
            options={[
              { value: 'atmos', label: 'Dolby Atmos (Tier 28)' },
              { value: 'hiRes192', label: 'Hi-Res FLAC 24-bit / 192 kHz' },
              { value: 'hiRes96', label: 'Hi-Res FLAC 24-bit / 96 kHz' },
              { value: 'cdFlac', label: 'CD Lossless FLAC 16-bit' },
              { value: 'high', label: 'Opus High (~256 kbps)' },
              { value: 'auto', label: 'Automatic' },
            ]}
          />
        </div>
        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Loudness Normalization</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">ReplayGain / EBU R128 LUFS target</div>
          </div>
          <Select
            bind:value={settingsStore.loudnessMode}
            options={[
              { value: 'off', label: 'Off (Bit-Perfect)' },
              { value: 'track', label: 'Track Gain' },
              { value: 'album', label: 'Album Gain' },
              { value: 'lufs', label: 'LUFS (-14 dB)' },
            ]}
          />
        </div>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">{audioCopy.exclusiveTitle}</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">{audioCopy.exclusiveDescription}</div>
          </div>
          <input
            type="checkbox"
            checked={settingsStore.bitPerfectEnabled && settingsStore.exclusiveMode}
            onchange={(e) => toggleBitPerfect(e.currentTarget.checked)}
            class="h-4 w-4 accent-[var(--lw-accent)]"
          />
        </label>
      </div>
    {:else if activeSection === 'downloads'}
      <DownloadLayoutSettings />
    {:else if activeSection === 'library'}
      <div class="space-y-4">
        <LibrarySettings />
        <BackupSettings />
        <CacheSettings />
      </div>
    {:else if activeSection === 'lyrics'}
      <div class="space-y-3">
        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Lyrics Provider</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Auto races all 16 providers and prioritizes word-synced karaoke</div>
          </div>
          <Select
            bind:value={featuresStore.lyricsProviderId}
            options={LYRICS_PROVIDERS.map((p) => ({ value: p.id, label: p.label }))}
            onchange={(v) => {
              settingsStore.saveStringPref('lw_lyrics_provider', v);
              if (playerStore.currentTrack) {
                void featuresStore.fetchLyricsForTrack(playerStore.currentTrack, { forceRefresh: true, providerId: v });
              }
            }}
          />
        </div>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Word-by-Word Karaoke Sync</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Progressive left-to-right syllable sweep animation</div>
          </div>
          <input
            type="checkbox"
            bind:checked={featuresStore.wordByWord}
            onchange={() => settingsStore.saveBoolPref('lw_word_by_word', featuresStore.wordByWord)}
            class="h-4 w-4 accent-[var(--lw-accent)]"
          />
        </label>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Show Romanization / Transliteration</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Display phonetic reading under non-Latin lyrics</div>
          </div>
          <input type="checkbox" bind:checked={featuresStore.showRomanized} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
        <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div class="flex justify-between text-[13px]">
            <span class="font-semibold">Lyrics Font Scale</span>
            <span class="lw-tabular">{Math.round(featuresStore.lyricsFontScale * 100)}%</span>
          </div>
          <input type="range" min="0.8" max="1.6" step="0.05" bind:value={featuresStore.lyricsFontScale} class="mt-2 h-1.5 w-full cursor-pointer accent-[var(--lw-accent)]" />
        </div>
      </div>
    {:else if activeSection === 'appearance'}
      <div class="space-y-4">
        <div class="space-y-2.5 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div class="text-[13.5px] font-semibold">Theme Mode</div>
          <div class="flex flex-wrap gap-2">
            {#each (['dark', 'amoled', 'light', 'custom', 'artwork'] as const) as mode (mode)}
              <Buttons variant="chip" size="sm" active={settingsStore.themeMode === mode} onclick={() => setThemeMode(mode)}>
                {mode === 'dark' ? 'Midnight' : mode === 'amoled' ? 'AMOLED' : mode === 'light' ? 'Pearl' : mode === 'custom' ? 'Custom' : 'Artwork'}
              </Buttons>
            {/each}
          </div>

          <div class="pt-2 text-[12px] text-[var(--lw-text-secondary)]">Custom Base Presets (WCAG AA)</div>
          <div class="flex flex-wrap gap-2">
            {#each CUSTOM_THEME_PRESETS as preset (preset.name)}
              <button
                type="button"
                onclick={() => setCustomBase(preset.color)}
                title={preset.name}
                class="h-7 w-7 cursor-pointer rounded-full border border-white/20 transition-transform hover:scale-110"
                style:background={preset.color}
              ></button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Accent Source</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Monochrome off-white, system, artwork, or custom</div>
          </div>
          <Select
            bind:value={settingsStore.accentSource}
            options={[
              { value: 'lastwave', label: 'LastWave Neutral' },
              { value: 'artwork', label: 'Artwork Tint' },
              { value: 'system', label: 'System Accent' },
              { value: 'custom', label: 'Custom Colour' },
            ]}
            onchange={() => syncThemeDocument()}
          />
        </div>

        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Glassmorphism</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Translucent surfaces over an app-wide artwork aura</div>
          </div>
          <input
            type="checkbox"
            bind:checked={settingsStore.glassmorphism}
            onchange={() => syncThemeDocument()}
            class="h-4 w-4 accent-[var(--lw-accent)]"
          />
        </label>

        <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">App Font</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">System default or typography preset</div>
          </div>
          <Select
            bind:value={settingsStore.appFont}
            options={APP_FONTS.map((f) => ({ value: f.id, label: f.label }))}
            onchange={() => syncThemeDocument()}
          />
        </div>

        <div class="space-y-2">
          <div class="text-[13.5px] font-semibold">Now Playing Style</div>
          <NowPlayingStylePicker />
        </div>
      </div>
    {:else if activeSection === 'plugins'}
      <PluginsSettings />
    {:else if activeSection === 'lastfm'}
      <ScrobblerSettings onOpenGuide={() => (keyGuideOpen = true)} />
    {:else if activeSection === 'sources'}
      <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4 text-[13px] text-[var(--lw-text-secondary)]">
        Stream waterfall: Lossless FLAC 24-bit / Dolby Atmos Addons → YouTube Music (VISIONOS / Opus) → Local Library.
      </div>
    {:else if activeSection === 'experimental'}
      <div class="space-y-3">
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Liquid Glass (Extra Translucency)</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Thinner surface fill with wider blur sigma</div>
          </div>
          <input type="checkbox" bind:checked={settingsStore.liquidGlass} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
        <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div>
            <div class="text-[13.5px] font-semibold">Wavy Seekbar</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">Draw dock playback timeline as a styled progress bar</div>
          </div>
          <input type="checkbox" bind:checked={settingsStore.wavySeekbar} class="h-4 w-4 accent-[var(--lw-accent)]" />
        </label>
      </div>
    {:else if activeSection === 'about'}
      <div class="space-y-4">
        <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
          <div class="text-[15px] font-bold">LastWave Desktop · Firebird UI</div>
          <div class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">Version 2.6.0 · Svelte 5 + Tailwind CSS v4</div>
        </div>
      </div>
    {/if}
    </div>
  </div>
</div>

<LastfmKeyGuide open={keyGuideOpen} onClose={() => (keyGuideOpen = false)} />
