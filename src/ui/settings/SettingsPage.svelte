<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { getPlatformAudioCopy } from '$src/features/audio_output/platformAudio';
  import { APP_FONTS } from '$src/features/settings/appFonts.svelte';
  import { setCustomBase, setThemeMode, syncThemeDocument } from '$src/features/settings/themeController.svelte';
  import { setUiScale, UI_SCALE_OPTIONS } from '$src/features/settings/uiScale.svelte';
  import NowPlayingStylePicker from '$src/features/settings/widgets/NowPlayingStylePicker.svelte';
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore, LYRICS_PROVIDERS } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore, THEME_MODES } from '$src/state/settings.svelte';
  import LastfmKeyGuide from '$src/ui/auth/LastfmKeyGuide.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import { CUSTOM_THEME_PRESETS } from '$src/ui/theme/tokens';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import BackupSettings from './BackupSettings.svelte';
  import CacheSettings from './CacheSettings.svelte';
  import DownloadLayoutSettings from './DownloadLayoutSettings.svelte';
  import LibrarySettings from './LibrarySettings.svelte';
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
    { id: 'appearance', label: 'Appearance', icon: 'palette' },
    { id: 'audio', label: 'Audio & Playback', icon: 'speaker' },
    { id: 'lyrics', label: 'Lyrics', icon: 'lyrics' },
    { id: 'integrations', label: 'Accounts & Scrobblers', icon: 'globe' },
    { id: 'library', label: 'Library & Downloads', icon: 'library' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

  const QUALITY_MAP: Record<string, number> = {
    atmos: 28,
    hiRes192: 27,
    hiRes96: 7,
    cdFlac: 6,
    high: -1,
    auto: -1,
  };

  let activeSection = $state('general');
  let query = $state('');
  let keyGuideOpen = $state(false);
  let verboseLogs = $state(false);
  let exportingLogs = $state(false);

  $effect(() => {
    void ipcInvoke<boolean>('diagnostics_is_verbose').then((v) => {
      if (typeof v === 'boolean') verboseLogs = v;
    });
  });

  async function exportLogs() {
    if (exportingLogs) return;
    exportingLogs = true;
    try {
      const savedPath = await ipcInvoke<string>('diagnostics_export_logs');
      featuresStore.showToast({
        title: 'Diagnostic logs exported',
        subtitle: savedPath ?? 'Saved zip bundle to Downloads',
        severity: 'success',
      });
    } catch (err) {
      featuresStore.showToast({
        title: 'Log export failed',
        subtitle: err instanceof Error ? err.message : 'Could not create diagnostic archive',
        severity: 'error',
      });
    } finally {
      exportingLogs = false;
    }
  }

  $effect(() => {
    const alias =
      initialSection === 'playback'
        ? 'audio'
        : initialSection === 'lastfm'
          ? 'integrations'
          : initialSection === 'downloads'
            ? 'library'
            : initialSection === 'experimental'
              ? 'appearance'
              : initialSection;
    if (alias && sections.some((s) => s.id === alias)) {
      activeSection = alias;
    }
  });

  let searchHits = $derived(searchSettings(query, settingsStore.platform));
  let audioCopy = $derived(getPlatformAudioCopy(settingsStore.platform));

  function toggleBitPerfect(enabled: boolean) {
    settingsStore.bitPerfectEnabled = enabled;
    settingsStore.exclusiveMode = enabled;
    void ipcInvoke('audio_output_set_bit_perfect', { enabled });
    void ipcInvoke('audio_output_set_exclusive', { exclusive: enabled });
  }

  function updateQualityTier(tier: string) {
    settingsStore.qualityTier = tier as typeof settingsStore.qualityTier;
    const code = QUALITY_MAP[tier] ?? -1;
    settingsStore.saveIntPref('lw_lossless_quality', code);
    settingsStore.saveBoolPref('lw_prefer_lossless_streaming', code >= 6);
    void ipcInvoke('player_apply_audio_prefs');
  }

  function updateLoudness(mode: string) {
    settingsStore.loudnessMode = mode as typeof settingsStore.loudnessMode;
    settingsStore.saveStringPref('lw_loudness_mode', mode);
    void ipcInvoke('player_apply_audio_prefs');
  }

  function updateAutoMix(enabled: boolean) {
    playerStore.automixEnabled = enabled;
    settingsStore.saveBoolPref('lw_automix_enabled', enabled);
    void ipcInvoke('player_refresh_overlap_prefs');
  }

  function updateCrossfade(sec: number) {
    playerStore.crossfadeSec = sec;
    settingsStore.saveBoolPref('lw_crossfade_enabled', sec > 0);
    if (sec > 0) settingsStore.saveIntPref('lw_crossfade_seconds', sec);
    void ipcInvoke('player_refresh_overlap_prefs');
  }

  function toggleDiscordRpc(enabled: boolean) {
    settingsStore.discordRpcEnabled = enabled;
    settingsStore.saveBoolPref('lw_discord_rich_presence', enabled);
    if (!enabled) void ipcInvoke('discord_presence_shutdown');
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
            class="flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-3 text-[13px] transition-all duration-[160ms] ease-[var(--ease-native)] {active
              ? 'bg-[var(--lw-wash-pressed)] font-semibold text-[var(--lw-text-primary)]'
              : 'text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
          >
            <WaveIcon name={sec.icon} size={16} />
            <span class="truncate">{sec.label}</span>
          </button>
        {/each}
      </nav>
    </aside>

    <div class="settings-section lw-scroll min-w-0 max-w-[780px] flex-1 space-y-5 pb-12 pr-2">
      <h2 class="text-[20px] font-bold tracking-[-0.3px]">
        {sections.find((s) => s.id === activeSection)?.label ?? 'Settings'}
      </h2>

      {#if activeSection === 'general'}
        <div class="space-y-3">
          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Language</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Interface display language</div>
            </div>
            <Select
              label="Language"
              value={settingsStore.language}
              options={[
                { value: 'system', label: 'System default' },
                { value: 'en', label: 'English' },
                { value: 'es', label: 'Español' },
                { value: 'ja', label: '日本語' },
              ]}
              onchange={(v) => {
                settingsStore.language = v;
                void ipcInvoke('language_set', { tag: v });
              }}
            />
          </div>

          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">UI Scale</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Adjust desktop interface density</div>
            </div>
            <Select
              label="UI Scale"
              value={String(settingsStore.uiScale)}
              options={UI_SCALE_OPTIONS.map((v) => ({ value: String(v), label: `${Math.round(v * 100)}%` }))}
              onchange={(v) => {
                const n = Number(v);
                setUiScale(n);
                settingsStore.saveDoublePref('lw_ui_scale', n);
              }}
            />
          </div>

          <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Diagnostic Logs</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Export redacted playback, mpv, and network session logs (.zip)</div>
            </div>
            <div class="flex items-center gap-3">
              <label class="flex cursor-pointer items-center gap-1.5 text-[12px] text-[var(--lw-text-secondary)]">
                <Switch
                  checked={verboseLogs}
                  onchange={(checked) => {
                    verboseLogs = checked;
                    void ipcInvoke('diagnostics_set_verbose', { verbose: verboseLogs });
                  }}
                />
                Verbose
              </label>
              <Buttons variant="secondary" size="sm" disabled={exportingLogs} onclick={exportLogs}>
                {exportingLogs ? 'Exporting...' : 'Export Logs'}
              </Buttons>
            </div>
          </div>
        </div>
      {:else if activeSection === 'appearance'}
        <div class="space-y-4">
          <div class="space-y-2.5 rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div class="text-[13.5px] font-semibold">Theme Mode</div>
            <div class="flex flex-wrap gap-2">
              {#each THEME_MODES as mode (mode.value)}
                <Buttons
                  variant="chip"
                  size="sm"
                  active={settingsStore.themeMode === mode.value}
                  onclick={() => setThemeMode(mode.value)}
                >
                  {mode.label}
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

          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Accent Source</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Theme default, system, artwork, or custom</div>
            </div>
            <Select
              label="Accent Source"
              value={settingsStore.accentSource}
              options={[
                { value: 'lastwave', label: 'Theme Default' },
                { value: 'artwork', label: 'Artwork Tint' },
                { value: 'system', label: 'System Accent' },
                { value: 'custom', label: 'Custom Colour' },
              ]}
              onchange={(v) => {
                settingsStore.accentSource = v as typeof settingsStore.accentSource;
                settingsStore.saveStringPref('lw_accent_mode', v);
                settingsStore.saveStringPref('lw_accent_source', v);
                syncThemeDocument();
              }}
            />
          </div>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Glassmorphism</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Translucent surfaces over an app-wide artwork aura</div>
            </div>
            <Switch
              checked={settingsStore.glassmorphism}
              onchange={(checked) => {
                settingsStore.glassmorphism = checked;
                settingsStore.saveBoolPref('lw_glassmorphism', settingsStore.glassmorphism);
                syncThemeDocument();
              }}
            />
          </label>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Liquid Glass</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Thinner surface fill with wider blur sigma</div>
            </div>
            <Switch
              checked={settingsStore.liquidGlass}
              onchange={(checked) => {
                settingsStore.liquidGlass = checked;
                settingsStore.saveBoolPref('lw_liquid_glass', settingsStore.liquidGlass);
                syncThemeDocument();
              }}
            />
          </label>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Wavy Seekbar</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Draw dock playback timeline as a styled progress bar</div>
            </div>
            <Switch
              checked={settingsStore.wavySeekbar}
              onchange={(checked) => {
                settingsStore.wavySeekbar = checked;
                settingsStore.saveBoolPref('lw_wavy_seekbar', settingsStore.wavySeekbar);
              }}
            />
          </label>

          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">App Font</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">System default or typography preset</div>
            </div>
            <Select
              label="App Font"
              value={settingsStore.appFont}
              options={APP_FONTS.map((f) => ({ value: f.id, label: f.label }))}
              onchange={(v) => {
                settingsStore.appFont = v;
                settingsStore.saveStringPref('lw_app_font', v);
                syncThemeDocument();
              }}
            />
          </div>

          <div class="space-y-2">
            <div class="text-[13.5px] font-semibold">Now Playing Style</div>
            <NowPlayingStylePicker />
          </div>
        </div>
      {:else if activeSection === 'audio'}
        <div class="space-y-3">
          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Preferred Quality Waterfall</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Stepping down cleanly when unavailable</div>
            </div>
            <Select
              label="Stream quality"
              value={settingsStore.qualityTier}
              options={[
                { value: 'atmos', label: 'Dolby Atmos (Tier 28)' },
                { value: 'hiRes192', label: 'Hi-Res FLAC 24-bit / 192 kHz' },
                { value: 'hiRes96', label: 'Hi-Res FLAC 24-bit / 96 kHz' },
                { value: 'cdFlac', label: 'CD Lossless FLAC 16-bit' },
                { value: 'high', label: 'Opus High (~256 kbps)' },
                { value: 'auto', label: 'Automatic' },
              ]}
              onchange={updateQualityTier}
            />
          </div>

          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Loudness Normalization</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">ReplayGain / EBU R128 LUFS target</div>
            </div>
            <Select
              label="Loudness normalization"
              value={settingsStore.loudnessMode}
              options={[
                { value: 'off', label: 'Off (Bit-Perfect)' },
                { value: 'track', label: 'Track Gain' },
                { value: 'album', label: 'Album Gain' },
                { value: 'lufs', label: 'LUFS (-14 dB)' },
              ]}
              onchange={updateLoudness}
            />
          </div>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">{audioCopy.exclusiveTitle}</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">{audioCopy.exclusiveDescription}</div>
            </div>
            <Switch
              checked={settingsStore.bitPerfectEnabled && settingsStore.exclusiveMode}
              onchange={(checked) => toggleBitPerfect(checked)}
            />
          </label>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Beat-Aware AutoMix</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Phase-lock beat grids during transitions</div>
            </div>
            <Switch
              checked={playerStore.automixEnabled}
              onchange={(checked) => updateAutoMix(checked)}
            />
          </label>

          <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div class="flex justify-between text-[13px]">
              <span class="font-semibold">Perceptual Crossfade</span>
              <span class="lw-tabular">{playerStore.crossfadeSec}s</span>
            </div>
            <input
              type="range"
              min="0"
              max="12"
              step="1"
              value={playerStore.crossfadeSec}
              onchange={(e) => updateCrossfade(Number(e.currentTarget.value))}
              class="mt-2 h-1.5 w-full cursor-pointer accent-[var(--lw-accent)]"
            />
          </div>
        </div>
      {:else if activeSection === 'lyrics'}
        <div class="space-y-3">
          <div class="settings-select-row rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Lyrics Provider</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Auto races all 16 providers and prioritizes word-synced karaoke</div>
            </div>
            <Select
              label="Lyrics provider"
              value={featuresStore.lyricsProviderId}
              options={LYRICS_PROVIDERS.map((p) => ({ value: p.id, label: p.label }))}
              onchange={(v) => {
                featuresStore.lyricsProviderId = v;
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
            <Switch
              checked={featuresStore.wordByWord}
              onchange={(checked) => {
                featuresStore.wordByWord = checked;
                settingsStore.saveBoolPref('lw_word_by_word', featuresStore.wordByWord);
              }}
            />
          </label>

          <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[13.5px] font-semibold">Show Romanization / Transliteration</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">Display phonetic reading under non-Latin lyrics</div>
            </div>
            <Switch
              checked={featuresStore.showRomanized}
              onchange={(checked) => {
                featuresStore.showRomanized = checked;
                settingsStore.saveBoolPref('lw_lyrics_transliteration', featuresStore.showRomanized);
              }}
            />
          </label>

          <div class="rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div class="flex justify-between text-[13px]">
              <span class="font-semibold">Lyrics Font Scale</span>
              <span class="lw-tabular">{Math.round(featuresStore.lyricsFontScale * 100)}%</span>
            </div>
            <input
              type="range"
              min="0.85"
              max="1.6"
              step="0.05"
              value={featuresStore.lyricsFontScale}
              onchange={(e) => {
                const v = Number(e.currentTarget.value);
                featuresStore.lyricsFontScale = v;
                settingsStore.saveDoublePref('lw_lyrics_text_scale', v);
              }}
              class="mt-2 h-1.5 w-full cursor-pointer accent-[var(--lw-accent)]"
            />
          </div>
        </div>
      {:else if activeSection === 'integrations'}
        <div class="space-y-4">
          <div class="space-y-3">
            <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">Music Accounts</div>
            <YtProfileChooser />
            <SpotifyConnect />
          </div>

          <div class="space-y-3">
            <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">Scrobblers</div>
            <ScrobblerSettings onOpenGuide={() => (keyGuideOpen = true)} />
          </div>

          <div class="space-y-3">
            <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">Desktop Presence</div>
            <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
              <div>
                <div class="text-[13.5px] font-semibold">Discord Rich Presence</div>
                <div class="text-[12px] text-[var(--lw-text-secondary)]">Show active track on your Discord profile</div>
              </div>
              <Switch
                checked={settingsStore.discordRpcEnabled}
                onchange={(checked) => toggleDiscordRpc(checked)}
              />
            </label>
            <label class="flex cursor-pointer items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
              <div>
                <div class="text-[13.5px] font-semibold">{audioCopy.mediaControlsTitle}</div>
                <div class="text-[12px] text-[var(--lw-text-secondary)]">Expose transport controls and timeline to the OS</div>
              </div>
              <Switch
                checked={settingsStore.smtcEnabled}
                onchange={(checked) => {
                  settingsStore.smtcEnabled = checked;
                  settingsStore.saveBoolPref('lw_macos_now_playing', settingsStore.smtcEnabled);
                }}
              />
            </label>
          </div>
        </div>
      {:else if activeSection === 'library'}
        <div class="space-y-4">
          <LibrarySettings />
          <DownloadLayoutSettings />
          <BackupSettings />
          <CacheSettings />
        </div>
      {:else if activeSection === 'about'}
        <div class="space-y-4">
          <div class="flex items-center justify-between rounded-[14px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] p-4">
            <div>
              <div class="text-[15px] font-bold">LastWave Desktop</div>
              <div class="mt-1 text-[12.5px] text-[var(--lw-text-secondary)]">Version 2.6.0</div>
              <div class="mt-3 flex flex-wrap gap-x-3 gap-y-1 text-[12px] text-[var(--lw-text-secondary)]">
                <span>Lyrics renderer · am-lyrics 1.9.2 by binimum</span>
                <a class="underline hover:text-[var(--lw-accent)]" href="/licenses/am-lyrics.txt" target="_blank" rel="noopener noreferrer">MPL-2.0 license</a>
                <a class="underline hover:text-[var(--lw-accent)]" href="https://registry.npmjs.org/@uimaxbai/am-lyrics/-/am-lyrics-1.9.2.tgz" target="_blank" rel="noopener noreferrer">am-lyrics source archive</a>
              </div>
            </div>
            <Buttons variant="secondary" size="sm" onclick={() => (windowState.updateDialogOpen = true)}>
              Check for Updates
            </Buttons>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<LastfmKeyGuide open={keyGuideOpen} onClose={() => (keyGuideOpen = false)} />

<style>
  .settings-section { container-type: inline-size; }
  .settings-select-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) min(47%, 248px);
    align-items: center;
    gap: 14px;
  }
  .settings-select-row > :global(.lw-select) { inline-size: 100%; }
  @container (max-width: 390px) {
    .settings-select-row { grid-template-columns: minmax(0, 1fr); gap: 9px; }
  }
</style>
