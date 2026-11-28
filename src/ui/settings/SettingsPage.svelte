<script lang="ts">
  import { tick } from 'svelte';
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { windowState } from '$src/app/window.svelte';
  import { getPlatformAudioCopy } from '$src/features/audio_output/platformAudio';
  import EqualizerPanel from '$src/features/equalizer/EqualizerPanel.svelte';
  import { resolveAccent } from '$src/features/settings/accent.svelte';
  import {
    APP_FONTS,
    ensureWebFontLoaded,
    fontFamilyFor,
    isWebFont,
  } from '$src/features/settings/appFonts.svelte';
  import {
    setCustomBase,
    setThemeMode,
    syncThemeDocument,
  } from '$src/features/settings/themeController.svelte';
  import { setUiScale, UI_SCALE_OPTIONS } from '$src/features/settings/uiScale.svelte';
  import NowPlayingStylePicker from '$src/features/settings/widgets/NowPlayingStylePicker.svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import {
    featuresStore,
    LYRICS_ANIMATION_OPTIONS,
    LYRICS_PROVIDERS,
  } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import {
    settingsStore,
    type AccentSource,
    type AmbientStyle,
    type HazeIntensity,
    type HazeMaterial,
    type MiniPlayerStyle,
  } from '$src/state/settings.svelte';
  import LastfmKeyGuide from '$src/ui/auth/LastfmKeyGuide.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import ColorPickerDialog from '$src/ui/components/ColorPickerDialog.svelte';
  import Input from '$src/ui/components/Input.svelte';
  import Select from '$src/ui/components/Select.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import WaveSlider from '$src/ui/components/WaveSlider.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import LibraryFolders from '$src/ui/library/LibraryFolders.svelte';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import BrandIcon from '$src/ui/theme/BrandIcon.svelte';
  import {
    CUSTOM_THEME_PRESETS,
    isLightBase,
    onAccentFor,
    surfacesFromCustomBase,
    WaveColors,
  } from '$src/ui/theme/tokens';
  import WaveIcon, { type WaveIconName } from '$src/ui/theme/WaveIcon.svelte';
  import BackupSettings from './BackupSettings.svelte';
  import CacheSettings from './CacheSettings.svelte';
  import DownloadLayoutSettings from './DownloadLayoutSettings.svelte';
  import LibrarySettings from './LibrarySettings.svelte';
  import ScrobblerSettings from './ScrobblerSettings.svelte';
  import {
    searchSettings,
    sectionDisplayLabel,
    type SettingsSearchEntry,
  } from './settingsSearch';
  import SpotifyConnect from './SpotifyConnect.svelte';
  import YtProfileChooser from './YtProfileChooser.svelte';

  interface Props {
    initialSection?: string;
  }

  let { initialSection = 'general' }: Props = $props();

  // Exact 12 canonical sections matching waveSettingsSections in settings_page.dart
  const SECTIONS: ReadonlyArray<{ id: string; label: string; icon: WaveIconName }> = [
    { id: 'general', label: 'General', icon: 'settings' },
    { id: 'integrations', label: 'Integrations', icon: 'link' },
    { id: 'playback', label: 'Playback', icon: 'play' },
    { id: 'audio', label: 'Audio', icon: 'speaker' },
    { id: 'downloads', label: 'Downloads', icon: 'download' },
    { id: 'library', label: 'Library', icon: 'library' },
    { id: 'lyrics', label: 'Lyrics', icon: 'artist' },
    { id: 'appearance', label: 'Appearance', icon: 'brush' },
    { id: 'lastfm', label: 'Scrobbler', icon: 'heart' },
    { id: 'sources', label: 'Sources', icon: 'cloud' },
    { id: 'experimental', label: 'Experimental', icon: 'bug' },
    { id: 'about', label: 'About', icon: 'info' },
  ];

  const ACCENT_PRESETS = [
    '#F5F4F0',
    '#4CC2FF',
    '#E03030',
    '#7C4DFF',
    '#2196C6',
    '#6B9E6B',
    '#E0A030',
    '#E0507A',
  ] as const;

  const APP_LANGUAGES = [
    { value: 'system', label: 'System default', sub: 'Follows your operating system language' },
    { value: 'en', label: 'English', sub: 'English' },
    { value: 'tr', label: 'Türkçe — Turkish', sub: 'Türkçe (Turkish)' },
    { value: 'zh-CN', label: '简体中文 — Chinese (Simplified)', sub: '简体中文 (Chinese Simplified)' },
    { value: 'ru', label: 'Русский — Russian', sub: 'Русский (Russian)' },
    { value: 'pt-BR', label: 'Português (Brasil) — Portuguese (Brazil)', sub: 'Português (Brazil)' },
    { value: 'es', label: 'Español — Spanish', sub: 'Español (Spanish)' },
    { value: 'id', label: 'Bahasa Indonesia — Indonesian', sub: 'Bahasa Indonesia (Indonesian)' },
    { value: 'hi', label: 'हिन्दी — Hindi', sub: 'हिन्दी (Hindi)' },
    { value: 'de', label: 'Deutsch — German', sub: 'Deutsch (German)' },
    { value: 'fr', label: 'Français — French', sub: 'Français (French)' },
    { value: 'it', label: 'Italiano — Italian', sub: 'Italiano (Italian)' },
    { value: 'ja', label: '日本語 — Japanese', sub: '日本語 (Japanese)' },
    { value: 'ko', label: '한국어 — Korean', sub: '한국어 (Korean)' },
    { value: 'ar', label: 'العربية — Arabic', sub: 'العربية (Arabic)' },
    { value: 'vi', label: 'Tiếng Việt — Vietnamese', sub: 'Tiếng Việt (Vietnamese)' },
  ] as const;

  const QUALITY_OPTIONS = [
    { value: 28, label: 'Dolby Atmos', desc: 'Spatial Audio · E-AC-3 JOC' },
    { value: 27, label: 'Hi-Res · 24/192', desc: 'FLAC up to 24-bit / 192 kHz' },
    { value: 7, label: 'Hi-Res · 24/96', desc: 'FLAC up to 24-bit / 96 kHz' },
    { value: 6, label: 'Lossless · 16/44.1', desc: 'CD-quality 16-bit / 44.1 kHz FLAC' },
    { value: 5, label: '320k MP3', desc: 'High bitrate lossy stream' },
    { value: -1, label: 'Opus · YouTube', desc: 'YouTube Music Opus (~160–256 kbps)' },
    { value: 4, label: 'Data saver', desc: 'Low bitrate stream for metered networks' },
  ] as const;

  const HEADPHONE_PROFILES = [
    { value: 'off', label: 'Off', hint: undefined },
    { value: 'sony-wh-1000xm5', label: 'Sony WH-1000XM5', hint: 'Harman over-ear curve' },
    { value: 'bose-qc-ultra', label: 'Bose QC Ultra', hint: 'Balanced ANC target' },
    { value: 'airpods-max', label: 'Apple AirPods Max', hint: 'Upper-mid studio trim' },
    { value: 'sennheiser-hd600', label: 'Sennheiser HD 600', hint: 'Sub-bass extension' },
    { value: 'ath-m50x', label: 'Audio-Technica ATH-M50x', hint: 'Tamed V-shape' },
    { value: 'dt-770', label: 'Beyerdynamic DT 770', hint: 'Treble peak smoothing' },
  ] as const;

  let activeSection = $state('general');
  let query = $state('');
  let highlightId = $state<string | null>(null);
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;
  let pageWidth = $state(1100);
  let narrow = $derived(pageWidth > 0 && pageWidth < 900);

  let keyGuideOpen = $state(false);
  let verboseLogs = $state(false);
  let exportingLogs = $state(false);

  // Color picker dialogs (Accent vs Custom Theme Base)
  let colorPickerMode = $state<'accent' | 'base' | null>(null);

  // Addon dialog state
  let addonDialog = $state<HTMLDialogElement | null>(null);
  let addonDraft = $state('');
  let addonError = $state('');

  // About update check state
  let checkingUpdates = $state(false);
  let upToDate = $state(false);
  let availableVersion = $state<string | null>(null);

  $effect(() => {
    if (initialSection && SECTIONS.some((s) => s.id === initialSection)) {
      activeSection = initialSection;
    }
  });

  $effect(() => {
    void ipcInvoke<boolean>('diagnostics_is_verbose').then((v) => {
      if (typeof v === 'boolean') verboseLogs = v;
    });
  });

  // Warm web fonts when Appearance opens so the font picker renders live previews
  $effect(() => {
    if (activeSection === 'appearance') {
      for (const f of APP_FONTS) {
        if (f.web && f.id) void ensureWebFontLoaded(f.id);
      }
    }
  });

  let searchHits = $derived(searchSettings(query, settingsStore.platform));
  let audioCopy = $derived(getPlatformAudioCopy(settingsStore.platform));
  let shortcutMod = $derived(settingsStore.platform === 'macos' ? 'Cmd' : 'Ctrl');

  let accentRes = $derived.by(() => {
    void settingsStore.accentSource;
    void settingsStore.accentColor;
    void settingsStore.themeMode;
    void settingsStore.customThemeBase;
    void playerStore.currentTrack?.accentColor;
    return resolveAccent();
  });

  let accentCaption = $derived.by(() => {
    const hex = accentRes.color.toUpperCase();
    switch (accentRes.source) {
      case 'custom':
        return `Custom colour · ${hex}`;
      case 'lastwave':
        return `LastWave neutral · ${hex}`;
      case 'system':
        return accentRes.resolved
          ? `Desktop accent · ${hex}`
          : 'This desktop exposes no accent — using custom colour';
      case 'artwork':
        return accentRes.resolved
          ? `Tinted from the current cover · ${hex}`
          : 'No cover hue to tint from — using custom colour';
    }
  });

  let isMidnight = $derived(
    settingsStore.themeMode === 'dark' || settingsStore.themeMode === 'amoled'
  );
  let isPresetAccent = $derived(
    ACCENT_PRESETS.some((c) => c.toLowerCase() === settingsStore.accentColor.toLowerCase())
  );
  let isPresetBase = $derived(
    CUSTOM_THEME_PRESETS.some(
      (p) => p.color.toLowerCase() === settingsStore.customThemeBase.toLowerCase()
    )
  );
  let customPreviewSurfaces = $derived(surfacesFromCustomBase(settingsStore.customThemeBase));
  let customPreviewLight = $derived(isLightBase(settingsStore.customThemeBase));

  function jumpToEntry(entry: SettingsSearchEntry) {
    activeSection = entry.section;
    highlightId = entry.id;
    query = '';
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightTimer = setTimeout(() => {
      highlightId = null;
    }, 2200);
    void tick().then(() => {
      const el = document.getElementById(`settings-group-${entry.id}`);
      el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
  }

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

  function openLogFolder() {
    void ipcInvoke('diagnostics_open_log_folder');
  }

  function updateStreamingQuality(code: number) {
    settingsStore.losslessQuality = code;
    settingsStore.qualityTier =
      code >= 28 ? 'atmos' : code >= 27 ? 'hiRes192' : code >= 7 ? 'hiRes96' : code >= 6 ? 'cdFlac' : 'high';
    settingsStore.saveIntPref('lw_lossless_quality', code);
    void ipcInvoke('player_apply_audio_prefs');
  }

  function updateDownloadQuality(code: number) {
    settingsStore.downloadQuality = code;
    settingsStore.saveIntPref('lw_download_quality', code);
  }

  function toggleExclusive(enabled: boolean) {
    settingsStore.exclusiveMode = enabled;
    settingsStore.bitPerfectEnabled = enabled;
    void ipcInvoke('audio_output_set_exclusive', { exclusive: enabled });
    void ipcInvoke('audio_output_set_bit_perfect', { enabled });
  }

  async function pickDownloadDir() {
    if (!isTauriRuntime()) return;
    const picked = await ipcInvoke<string | null>('dialog_pick_folder', {
      title: 'Choose download folder',
    }).catch(() => null);
    if (picked && picked.trim()) {
      settingsStore.downloadDir = picked.trim();
      settingsStore.saveStringPref('lw_download_dir', picked.trim());
    }
  }

  function resetDownloadDir() {
    settingsStore.downloadDir = '';
    void ipcInvoke('prefs_remove', { key: 'lw_download_dir' });
  }

  function persistAddonList(urls: string[]) {
    settingsStore.addonUrls = urls;
    settingsStore.saveStringPref('lw_addon_urls', JSON.stringify(urls));
  }

  function persistDisabledAddons(disabled: string[]) {
    settingsStore.disabledAddonUrls = disabled;
    settingsStore.saveStringPref('lw_disabled_addon_urls', JSON.stringify(disabled));
  }

  function submitNewAddon() {
    const raw = addonDraft.trim();
    if (!raw) return;
    try {
      const parsed = new URL(raw);
      if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
        addonError = 'Expected https://host/a/<token>/ or https://host/.';
        return;
      }
      const normalized = raw.endsWith('/') ? raw : `${raw}/`;
      const next = [...settingsStore.addonUrls.filter((u) => u !== normalized), normalized];
      persistAddonList(next);
      persistDisabledAddons(settingsStore.disabledAddonUrls.filter((u) => u !== normalized));
      if (!settingsStore.addonsEnabled) {
        settingsStore.addonsEnabled = true;
        settingsStore.saveBoolPref('lw_addons_enabled', true);
      }
      addonDraft = '';
      addonError = '';
      addonDialog?.close();
    } catch {
      addonError = 'Expected something like https://host/a/<token>/ or https://host/.';
    }
  }

  async function checkForUpdates() {
    if (checkingUpdates) return;
    checkingUpdates = true;
    upToDate = false;
    try {
      const info = await ipcInvoke<{ version?: string } | null>('updates_check').catch(() => null);
      if (info?.version) {
        availableVersion = info.version;
        windowState.updateDialogOpen = true;
      } else {
        availableVersion = null;
        upToDate = true;
      }
    } finally {
      checkingUpdates = false;
    }
  }

  function openExternal(url: string) {
    window.open(url, '_blank', 'noopener,noreferrer');
  }

  function qualitySubtitle(code: number): string {
    return QUALITY_OPTIONS.find((q) => q.value === code)?.desc ?? 'Lossless-first audio';
  }

  function loudnessSubtitle(mode: string): string {
    switch (mode) {
      case 'track':
        return 'Evens out volume song by song (-14 LUFS)';
      case 'album':
        return 'Keeps album dynamics intact while normalizing overall loudness';
      case 'lufs':
        return 'Targets EBU R128 -14 dB integrated loudness';
      default:
        return 'Off — untouched bit-exact gain';
    }
  }

  function lyricsAnimSubtitle(id: string): string {
    switch (id) {
      case 'karaoke_bounce':
        return 'Energetic rhythmic syllable bounce';
      case 'kinetic_pop':
        return 'Ultra-bold punchy type with spring-popping syllables';
      case 'neon_cyber':
        return 'Accent-lit cyberpunk neon glow with forward kinetic slant';
      case 'italic_wave':
        return 'Expressive luxury italic type rippling like a liquid wave';
      case 'glitch_snap':
        return 'RGB chromatic split with crisp digital snap-in pulses';
      case 'elastic_tilt':
        return '3D perspective stage lean with elastic overshoot motion';
      case 'hyper_warp':
        return 'Continuous 3D warp surge with shockwave rings and afterimages';
      case 'chaos_vortex':
        return '3D stage sway with rubber squash-and-stretch spin and prism echoes';
      default:
        return 'Smooth fluid syllable wipe with gentle micro-float';
    }
  }

  function miniPlayerSubtitle(style: MiniPlayerStyle): string {
    switch (style) {
      case 'compact':
        return 'Slim strip with just the essentials.';
      case 'card':
        return 'Square artwork card with controls over the cover.';
      case 'ambient':
        return 'Wide panel glowing with the blurred artwork and a timed scrubber.';
      case 'lyrics':
        return 'Ambient panel with space for the current synced lyric line.';
      default:
        return 'Bar with artwork, transport and window buttons.';
    }
  }
</script>

{#snippet groupHeader(title: string, subtitle: string)}
  <div class="lw-settings-group-head">
    <div class="text-[15px] font-bold leading-snug text-[var(--lw-text-primary)]">{title}</div>
    <div class="mt-0.5 text-[12px] leading-snug text-[var(--lw-text-secondary)]">{subtitle}</div>
  </div>
{/snippet}

{#snippet sectionContent()}
  {#if activeSection === 'general'}
    <div class="space-y-4">
      <!-- Behaviour -->
      <div id="settings-group-general.behaviour" class="lw-settings-group" data-spotlight={highlightId === 'general.behaviour'}>
        {@render groupHeader('Behaviour', 'Startup and window defaults')}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Close button minimize to tray.</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Keep the player running in the background when closed.
              </div>
            </div>
            <Switch
              label="Close button minimize to tray"
              checked={settingsStore.closeToTray}
              onchange={(v) => {
                settingsStore.closeToTray = v;
                settingsStore.saveBoolPref('lw_close_to_tray', v);
              }}
            />
          </div>

          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Miniplayer style</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {miniPlayerSubtitle(settingsStore.miniPlayerStyle)}
              </div>
            </div>
            <div class="w-[220px] shrink-0">
              <Select
                expanded
                label="Miniplayer style"
                value={settingsStore.miniPlayerStyle}
                options={[
                  { value: 'classic', label: 'Classic bar' },
                  { value: 'compact', label: 'Compact strip' },
                  { value: 'card', label: 'Artwork card' },
                  { value: 'ambient', label: 'Ambient wide' },
                  { value: 'lyrics', label: 'Ambient + lyrics' },
                ]}
                onchange={(v) => {
                  settingsStore.miniPlayerStyle = v as MiniPlayerStyle;
                  settingsStore.saveStringPref('lw_mini_player_style', v);
                }}
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Language -->
      <div id="settings-group-general.language" class="lw-settings-group" data-spotlight={highlightId === 'general.language'}>
        {@render groupHeader('Language', 'Select the interface and InnerTube language')}
        <div class="lw-settings-group-body">
          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Language</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {APP_LANGUAGES.find((l) => l.value === settingsStore.language)?.sub ?? 'System default'}
              </div>
            </div>
            <div class="w-[220px] shrink-0">
              <Select
                expanded
                label="Language"
                value={APP_LANGUAGES.some((l) => l.value === settingsStore.language)
                  ? settingsStore.language
                  : 'system'}
                options={APP_LANGUAGES}
                onchange={(v) => {
                  settingsStore.language = v;
                  void ipcInvoke('language_set', { tag: v });
                }}
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Backup -->
      <div id="settings-group-general.backup" class="lw-settings-group" data-spotlight={highlightId === 'general.backup'}>
        {@render groupHeader('Backup', 'Move your playlists and settings to another device')}
        <div class="lw-settings-group-body">
          <BackupSettings />
        </div>
      </div>

      <!-- Cached data -->
      <div id="settings-group-general.cache" class="lw-settings-group" data-spotlight={highlightId === 'general.cache'}>
        {@render groupHeader('Cached data', 'Pages saved on this device so they open instantly')}
        <div class="lw-settings-group-body">
          <CacheSettings />
        </div>
      </div>

      <!-- Diagnostics -->
      <div id="settings-group-general.diagnostics" class="lw-settings-group" data-spotlight={highlightId === 'general.diagnostics'}>
        {@render groupHeader('Diagnostics', 'Local logs for troubleshooting — nothing is uploaded')}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Verbose logging</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Adds per-request detail (stream clients, audio probes). Reproduce the problem with it on.
              </div>
            </div>
            <Switch
              label="Verbose logging"
              checked={verboseLogs}
              onchange={(v) => {
                verboseLogs = v;
                settingsStore.saveBoolPref('lw_verbose_logs', v);
                void ipcInvoke('diagnostics_set_verbose', { verbose: v });
              }}
            />
          </div>

          <div class="flex flex-wrap items-center gap-2">
            <Buttons variant="primary" size="sm" disabled={exportingLogs} onclick={() => void exportLogs()}>
              {exportingLogs ? 'Exporting…' : 'Export diagnostic logs'}
            </Buttons>
            <Buttons variant="secondary" size="sm" onclick={openLogFolder}>
              Open log folder
            </Buttons>
          </div>

          <p class="text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
            Packages the last 5 runs and the crash trail into one zip. URLs, cookies and tokens are stripped; track titles are kept so a wrong-song report stays actionable.
          </p>
        </div>
      </div>
    </div>

  {:else if activeSection === 'integrations'}
    <div class="space-y-4">
      <div id="settings-group-integrations.ytm" class="lw-settings-group" data-spotlight={highlightId === 'integrations.ytm'}>
        {@render groupHeader('YouTube Music', 'Personal library, history and uploads')}
        <div class="lw-settings-group-body">
          <YtProfileChooser />
        </div>
      </div>

      <div id="settings-group-integrations.spotify" class="lw-settings-group" data-spotlight={highlightId === 'integrations.spotify'}>
        {@render groupHeader('Spotify', 'Your playlists, Liked Songs and Canvas')}
        <div class="lw-settings-group-body">
          <SpotifyConnect />
        </div>
      </div>

      <div id="settings-group-integrations.discord" class="lw-settings-group" data-spotlight={highlightId === 'integrations.discord'}>
        {@render groupHeader('Discord', 'Show what you are listening to on your Discord profile')}
        <div class="lw-settings-group-body">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Discord Rich Presence</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Uses the local Discord client · no bot or login
              </div>
            </div>
            <Switch
              label="Discord Rich Presence"
              checked={settingsStore.discordRpcEnabled}
              onchange={(v) => {
                settingsStore.discordRpcEnabled = v;
                settingsStore.saveBoolPref('lw_discord_rich_presence', v);
                if (!v) void ipcInvoke('discord_presence_shutdown');
              }}
            />
          </div>
        </div>
      </div>
    </div>

  {:else if activeSection === 'playback'}
    <div id="settings-group-playback.main" class="lw-settings-group" data-spotlight={highlightId === 'playback.main'}>
      {@render groupHeader('Playback', 'Gapless, crossfade and output behaviour')}
      <div class="lw-settings-group-body space-y-3">
        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Autoplay similar</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Keep playing related tracks after a queue ends
            </div>
          </div>
          <Switch
            label="Autoplay similar"
            checked={settingsStore.autoplaySimilar}
            onchange={(v) => {
              settingsStore.autoplaySimilar = v;
              settingsStore.saveBoolPref('lw_autoplay_similar', v);
            }}
          />
        </div>

        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Canvas &amp; animated artwork</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Play looping Canvas, motion cover art, and artist banner videos
            </div>
          </div>
          <Switch
            label="Canvas & animated artwork"
            checked={settingsStore.canvasEnabled}
            onchange={(v) => {
              settingsStore.canvasEnabled = v;
              settingsStore.saveBoolPref('lw_canvas_enabled', v);
            }}
          />
        </div>

        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">{audioCopy.mediaControlsTitle}</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Show LastWave on the lock screen and enable system media controls
            </div>
          </div>
          <Switch
            label={audioCopy.mediaControlsTitle}
            checked={settingsStore.smtcEnabled}
            onchange={(v) => {
              settingsStore.smtcEnabled = v;
              settingsStore.saveBoolPref('lw_macos_now_playing', v);
            }}
          />
        </div>

        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Crossfade</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              {settingsStore.autoMixEnabled
                ? 'Off while timed AutoMix is on'
                : settingsStore.crossfadeEnabled
                  ? `${settingsStore.crossfadeSeconds}s crossfade duration`
                  : 'Gapless direct transition (crossfade disabled)'}
            </div>
          </div>
          <Switch
            label="Crossfade"
            checked={settingsStore.crossfadeEnabled}
            onchange={(v) => {
              settingsStore.crossfadeEnabled = v;
              playerStore.crossfadeSec = v ? settingsStore.crossfadeSeconds : 0;
              settingsStore.saveBoolPref('lw_crossfade_enabled', v);
              void ipcInvoke('player_refresh_overlap_prefs');
            }}
          />
        </div>

        {#if settingsStore.crossfadeEnabled}
          <div class="px-1 pt-1 pb-1">
            <div class="mb-1.5 flex items-center justify-between">
              <span class="text-[13.5px] font-semibold">Crossfade duration</span>
              <span class="text-[12px] font-semibold text-[var(--lw-accent)] lw-tabular">
                {settingsStore.crossfadeSeconds}s
              </span>
            </div>
            <WaveSlider
              min={3}
              max={12}
              step={1}
              label="Crossfade duration"
              valuetext={`${settingsStore.crossfadeSeconds} seconds`}
              value={settingsStore.crossfadeSeconds}
              oninput={(v) => {
                settingsStore.crossfadeSeconds = v;
                playerStore.crossfadeSec = v;
              }}
              onchange={(v) => {
                settingsStore.saveIntPref('lw_crossfade_seconds', v);
                void ipcInvoke('player_refresh_overlap_prefs');
              }}
            />
            <div class="mt-1 flex justify-between text-[11.5px] text-[var(--lw-text-secondary)]">
              <span>3s (Short)</span>
              <span>7s (Standard)</span>
              <span>12s (Long)</span>
            </div>
          </div>
        {/if}
      </div>
    </div>

  {:else if activeSection === 'audio'}
    <div class="space-y-3">
      <!-- Audio output -->
      <div id="settings-group-audio.output" class="lw-settings-group" data-spotlight={highlightId === 'audio.output'}>
        {@render groupHeader(
          'Audio output',
          settingsStore.platform === 'windows'
            ? 'WASAPI Exclusive bypasses the Windows mixer'
            : 'System audio output'
        )}
        <div class="lw-settings-group-body space-y-3">
          <div>
            <div class="mb-1.5 text-[13.5px] font-semibold">Device</div>
            <Select
              expanded
              label="Output device"
              value={featuresStore.devices.some((d) => d.id === featuresStore.selectedDeviceId)
                ? featuresStore.selectedDeviceId
                : ''}
              options={[
                { value: '', label: 'System Default' },
                ...featuresStore.devices.map((d) => ({
                  value: d.id,
                  label: d.name,
                  hint: d.isDefault ? 'default' : undefined,
                })),
              ]}
              onchange={(id) => featuresStore.selectDevice(id)}
            />
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">{audioCopy.exclusiveTitle}</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {settingsStore.exclusiveMode && !featuresStore.eqEnabled
                  ? 'Mixer bypassed · bit-perfect when the DAC matches the source'
                  : settingsStore.exclusiveMode && featuresStore.eqEnabled
                    ? 'DSP EQ active — disable EQ for bit-perfect pass-through'
                    : 'Shared mode — mixer may resample'}
              </div>
            </div>
            <Switch
              label={audioCopy.exclusiveTitle}
              checked={settingsStore.exclusiveMode}
              onchange={toggleExclusive}
            />
          </div>

          <div class="text-[12px] text-[var(--lw-text-secondary)]">
            {#if featuresStore.selectedDevice}
              {featuresStore.selectedDevice.driver} · up to {featuresStore.selectedDevice.maxBitDepth}-bit / {Math.round(featuresStore.selectedDevice.maxSampleRateHz / 1000)} kHz
            {:else}
              Probe the DAC after connecting it to see exclusive PCM rates.
            {/if}
          </div>

          <!-- Inline Stream Path Panel (matches WaveStreamPathPanel) -->
          <div class="rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3.5 py-2.5 text-[12px]">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <span class="font-semibold text-[var(--lw-text-primary)]">Signal path</span>
              <span
                class="rounded-full px-2 py-0.5 text-[10.5px] font-semibold {settingsStore.exclusiveMode && !featuresStore.eqEnabled
                  ? 'bg-[var(--lw-success)]/15 text-[var(--lw-success)]'
                  : 'bg-[var(--lw-surface-overlay)] text-[var(--lw-text-secondary)]'}"
              >
                {settingsStore.exclusiveMode && !featuresStore.eqEnabled ? 'Bit-Perfect' : 'Shared / Mixer'}
              </span>
            </div>
            <div class="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11.5px] text-[var(--lw-text-secondary)] lw-tabular">
              <span>
                {playerStore.currentTrack
                  ? `${playerStore.currentTrack.codec} · ${(playerStore.currentTrack.sampleRateHz / 1000).toFixed(1)} kHz`
                  : 'Idle'}
              </span>
              <span>→</span>
              <span>{featuresStore.eqEnabled || featuresStore.studioClarity ? '10-Band DSP' : 'Direct PCM'}</span>
              <span>→</span>
              <span>{featuresStore.selectedDevice?.name ?? 'System Default'}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Streaming & Download quality -->
      <div id="settings-group-audio.quality" class="lw-settings-group" data-spotlight={highlightId === 'audio.quality'}>
        {@render groupHeader('Streaming and Download quality', 'Lossless-first with YouTube fallback')}
        <div class="lw-settings-group-body space-y-3">
          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Streaming quality</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {qualitySubtitle(settingsStore.losslessQuality)}
              </div>
            </div>
            <div class="w-[200px] shrink-0">
              <Select
                expanded
                label="Streaming quality"
                value={QUALITY_OPTIONS.some((q) => q.value === settingsStore.losslessQuality)
                  ? settingsStore.losslessQuality
                  : 27}
                options={QUALITY_OPTIONS}
                onchange={updateStreamingQuality}
              />
            </div>
          </div>

          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Download quality</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {qualitySubtitle(settingsStore.downloadQuality)}
              </div>
            </div>
            <div class="w-[200px] shrink-0">
              <Select
                expanded
                label="Download quality"
                value={QUALITY_OPTIONS.some((q) => q.value === settingsStore.downloadQuality)
                  ? settingsStore.downloadQuality
                  : 27}
                options={QUALITY_OPTIONS}
                onchange={updateDownloadQuality}
              />
            </div>
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Prefer lossless</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Try lossless first, fall back to Opus
              </div>
            </div>
            <Switch
              label="Prefer lossless"
              checked={settingsStore.preferLossless}
              onchange={(v) => {
                settingsStore.preferLossless = v;
                settingsStore.saveBoolPref('lw_prefer_lossless_streaming', v);
                void ipcInvoke('player_apply_audio_prefs');
              }}
            />
          </div>
        </div>
      </div>

      <!-- Loudness -->
      <div id="settings-group-audio.loudness" class="lw-settings-group" data-spotlight={highlightId === 'audio.loudness'}>
        {@render groupHeader('Loudness', loudnessSubtitle(settingsStore.loudnessMode))}
        <div class="lw-settings-group-body">
          <div class="mb-1.5 text-[13.5px] font-semibold">Normalization</div>
          <Select
            label="Normalization"
            value={settingsStore.loudnessMode}
            options={[
              { value: 'off', label: 'Off' },
              { value: 'track', label: 'Track' },
              { value: 'album', label: 'Album' },
            ]}
            onchange={(mode) => {
              settingsStore.loudnessMode = mode as typeof settingsStore.loudnessMode;
              settingsStore.saveStringPref('lw_loudness_mode', mode);
              void ipcInvoke('player_apply_audio_prefs');
            }}
          />
        </div>
      </div>

      <!-- Headphone correction -->
      <div id="settings-group-audio.headphones" class="lw-settings-group" data-spotlight={highlightId === 'audio.headphones'}>
        {@render groupHeader('Headphone correction', 'A built-in curve for a few common headphones')}
        <div class="lw-settings-group-body">
          <Select
            expanded
            label="Headphone correction"
            value={HEADPHONE_PROFILES.some((p) => p.value === featuresStore.headphoneProfile)
              ? featuresStore.headphoneProfile
              : 'off'}
            options={HEADPHONE_PROFILES}
            onchange={(id) => {
              featuresStore.headphoneProfile = id;
              settingsStore.saveStringPref('lw_headphone_profile', id);
              void ipcInvoke('player_apply_audio_prefs');
            }}
          />
        </div>
      </div>

      <!-- AutoMix -->
      <div id="settings-group-audio.automix" class="lw-settings-group" data-spotlight={highlightId === 'audio.automix'}>
        {@render groupHeader(
          'AutoMix',
          settingsStore.autoMixEnabled
            ? 'DJ-style blends on the beat. Turns crossfade off.'
            : 'Off. Songs change without a blend.'
        )}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Timed mix</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {settingsStore.autoMixEnabled
                  ? `Blends each ending into the next song over ${settingsStore.autoMixBars} bars, on the beat, with a DJ bass handover. Nothing is cut. Album tracks stay gapless.`
                  : 'Gapless until you turn this on'}
              </div>
            </div>
            <Switch
              label="Timed mix"
              checked={settingsStore.autoMixEnabled}
              onchange={(v) => {
                settingsStore.autoMixEnabled = v;
                playerStore.automixEnabled = v;
                if (v) settingsStore.crossfadeEnabled = false;
                settingsStore.saveBoolPref('lw_automix_enabled', v);
                void ipcInvoke('player_refresh_overlap_prefs');
              }}
            />
          </div>

          {#if settingsStore.autoMixEnabled}
            <div>
              <div class="mb-2 text-[13.5px] font-semibold">Blend length</div>
              <div class="grid grid-cols-2 gap-2">
                {#each [8, 16] as bars (bars)}
                  <Buttons
                    variant="toggle"
                    active={settingsStore.autoMixBars === bars}
                    class="w-full"
                    onclick={() => {
                      settingsStore.autoMixBars = bars;
                      settingsStore.saveIntPref('lw_automix_bars', bars);
                      void ipcInvoke('player_refresh_overlap_prefs');
                    }}
                  >
                    {bars} bars
                  </Buttons>
                {/each}
              </div>
            </div>

            <div class="flex items-center justify-between gap-3 py-1">
              <div class="min-w-0 flex-1">
                <div class="text-[13.5px] font-semibold">Bass swap &amp; filters</div>
                <div class="text-[12px] text-[var(--lw-text-secondary)]">
                  Hands the bass over on the beat and crosses the highs, like a DJ mixer.
                </div>
              </div>
              <Switch
                label="Bass swap & filters"
                checked={settingsStore.autoMixSpectral}
                onchange={(v) => {
                  settingsStore.autoMixSpectral = v;
                  settingsStore.saveBoolPref('lw_automix_spectral', v);
                  void ipcInvoke('player_apply_audio_prefs');
                }}
              />
            </div>
          {/if}
        </div>
      </div>

      <!-- Equalizer & Sound FX -->
      <div id="settings-group-audio.equalizer" class="lw-settings-group" data-spotlight={highlightId === 'audio.equalizer'}>
        {@render groupHeader('Equalizer & Sound FX', '10-band ISO curves with acoustic & stage compensation')}
        <div class="lw-settings-group-body">
          <EqualizerPanel embedded={true} />
        </div>
      </div>
    </div>

  {:else if activeSection === 'downloads'}
    <div id="settings-group-downloads.offline" class="lw-settings-group" data-spotlight={highlightId === 'downloads.offline'}>
      {@render groupHeader('Offline', 'Download behaviour and lyrics sidecars')}
      <div class="lw-settings-group-body space-y-4">
        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Download lyrics</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Save synced .lrc sidecars with downloads
            </div>
          </div>
          <Switch
            label="Download lyrics"
            checked={settingsStore.embedLyrics}
            onchange={(v) => {
              settingsStore.embedLyrics = v;
              settingsStore.saveBoolPref('lw_download_lyrics', v);
            }}
          />
        </div>

        <div class="flex items-center gap-2.5">
          <WaveIcon name="folderOpen" size={18} class="shrink-0 text-[var(--lw-text-secondary)]" />
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Save location</div>
            <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
              {settingsStore.downloadDir || 'Default location'}
            </div>
          </div>
          <Buttons variant="secondary" size="sm" onclick={() => void pickDownloadDir()}>
            Change
          </Buttons>
          {#if settingsStore.downloadDir}
            <Buttons variant="secondary" size="sm" onclick={resetDownloadDir}>
              Reset
            </Buttons>
          {/if}
        </div>

        <DownloadLayoutSettings />
      </div>
    </div>

  {:else if activeSection === 'library'}
    <div class="space-y-4">
      <div id="settings-group-library.folders" class="lw-settings-group" data-spotlight={highlightId === 'library.folders'}>
        {@render groupHeader('Local folders', 'Music folders scanned automatically on this device')}
        <div class="lw-settings-group-body">
          <LibraryFolders />
        </div>
      </div>

      <div id="settings-group-library.sync" class="lw-settings-group" data-spotlight={highlightId === 'library.sync'}>
        {@render groupHeader('Library', 'Podcasts, YouTube Music sync, and hidden tracks')}
        <div class="lw-settings-group-body">
          <LibrarySettings />
        </div>
      </div>
    </div>

  {:else if activeSection === 'lyrics'}
    <div class="space-y-3">
      <div id="settings-group-lyrics.animation" class="lw-settings-group" data-spotlight={highlightId === 'lyrics.animation'}>
        {@render groupHeader('Lyrics animation', 'Visual animation style for synced lyrics in Now Playing')}
        <div class="lw-settings-group-body">
          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Animation style</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {lyricsAnimSubtitle(featuresStore.lyricsAnimation)}
              </div>
            </div>
            <div class="w-[220px] shrink-0">
              <Select
                expanded
                label="Animation style"
                value={featuresStore.lyricsAnimation}
                options={LYRICS_ANIMATION_OPTIONS.map((a) => ({ value: a.id, label: a.label }))}
                onchange={(v) => featuresStore.setLyricsAnimation(v)}
              />
            </div>
          </div>
        </div>
      </div>

      <div id="settings-group-lyrics.timing" class="lw-settings-group" data-spotlight={highlightId === 'lyrics.timing'}>
        {@render groupHeader('Timing', 'Synced lyrics providers')}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Word-by-word lyrics</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Karaoke word highlight. Off uses Apple Music line lyrics.
              </div>
            </div>
            <Switch
              label="Word-by-word lyrics"
              checked={featuresStore.wordByWord}
              onchange={(v) => featuresStore.setWordByWord(v)}
            />
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Romanisation</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Display phonetic reading under non-Latin lyrics
              </div>
            </div>
            <Switch
              label="Romanisation"
              checked={featuresStore.showRomanized}
              onchange={(v) => featuresStore.setLyricsRomanized(v)}
            />
          </div>

          <div>
            <div class="mb-1.5 text-[13.5px] font-semibold">Primary provider</div>
            <Select
              label="Primary provider"
              value={featuresStore.lyricsProviderId}
              options={LYRICS_PROVIDERS.map((p) => ({ value: p.id, label: p.label }))}
              onchange={(v) => featuresStore.setDefaultLyricsProvider(v)}
            />
            <p class="mt-1.5 text-[12px] text-[var(--lw-text-secondary)]">
              {LYRICS_PROVIDERS.find((p) => p.id === featuresStore.lyricsProviderId)?.subtitle ??
                'Fastest word-sync wins, LRCLIB fallback'}
            </p>
          </div>
        </div>
      </div>
    </div>

  {:else if activeSection === 'appearance'}
    {@const activePluginTheme =
      !pluginManager.safeMode && !pluginManager.allDisabled
        ? pluginManager.plugins.find((p) => p.manifest.id === pluginManager.activeThemeId)
        : null}
    {@const activePluginPalette =
      !pluginManager.safeMode && !pluginManager.allDisabled
        ? pluginManager.plugins.find((p) => p.manifest.id === pluginManager.activePaletteId)
        : null}

    <div class="space-y-3">
      {#if activePluginTheme || activePluginPalette}
        <div class="flex flex-wrap items-center justify-between gap-3 rounded-[16px] border border-[var(--lw-accent)]/40 bg-[var(--lw-accent-subtle)] px-4 py-3 text-[12.5px]">
          <div class="min-w-0 flex-1">
            <div class="font-semibold text-[var(--lw-text-primary)]">
              Plugin override active: {activePluginTheme?.manifest.name || activePluginPalette?.manifest.name}
            </div>
            <div class="mt-0.5 text-[11.5px] text-[var(--lw-text-secondary)]">
              Built-in Theme and Custom colours are overridden while a plugin Theme or Palette is enabled.
            </div>
          </div>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => {
              if (activePluginTheme) pluginManager.activateTheme(null);
              if (activePluginPalette) pluginManager.activatePalette(null);
            }}
          >
            Disable Override
          </Buttons>
        </div>
      {/if}

      <!-- 1. Theme (matches appearance.theme in settings_page.dart) -->
      <div
        id="settings-group-appearance.theme"
        class="lw-settings-group {activePluginTheme || activePluginPalette ? 'pointer-events-none opacity-45' : ''}"
        data-spotlight={highlightId === 'appearance.theme'}
      >
        {@render groupHeader('Theme', 'Midnight, Pearl, artwork, or your own colours')}
        <div class="lw-settings-group-body space-y-3.5">
          <!-- 4 ToggleButtons: Midnight · Pearl · Artwork · Custom -->
          <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
            {#each [
              { id: 'dark', label: 'Midnight', active: isMidnight },
              { id: 'light', label: 'Pearl', active: settingsStore.themeMode === 'light' },
              { id: 'artwork', label: 'Artwork', active: settingsStore.themeMode === 'artwork' },
              { id: 'custom', label: 'Custom', active: settingsStore.themeMode === 'custom' },
            ] as m (m.id)}
              <Buttons
                variant="toggle"
                active={m.active}
                class="w-full"
                onclick={() => setThemeMode(m.id as 'dark' | 'light' | 'artwork' | 'custom')}
              >
                {m.label}
              </Buttons>
            {/each}
          </div>

          <!-- Glassmorphism -->
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Glassmorphism</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                The whole app as frosted glass over the playing artwork · works with every theme
              </div>
            </div>
            <Switch
              label="Glassmorphism"
              checked={settingsStore.glassmorphism}
              onchange={(v) => {
                settingsStore.glassmorphism = v;
                settingsStore.saveBoolPref('lw_glassmorphism', v);
                syncThemeDocument();
              }}
            />
          </div>

          <!-- Custom Theme Editor + Miniature Shell Preview (matches _CustomThemeEditor & _ThemePreview) -->
          {#if settingsStore.themeMode === 'custom'}
            <div class="space-y-2.5 pt-1">
              <div class="text-[13.5px] font-semibold">Base colour</div>
              <div class="flex flex-wrap gap-2">
                {#each CUSTOM_THEME_PRESETS as preset (preset.name)}
                  {@const selected = preset.color.toLowerCase() === settingsStore.customThemeBase.toLowerCase()}
                  <button
                    type="button"
                    onclick={() => setCustomBase(preset.color)}
                    use:waveTooltip={preset.name}
                    aria-label={preset.name}
                    class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border-2 transition-transform hover:scale-105 {selected
                      ? 'border-[var(--lw-accent)]'
                      : 'border-[var(--lw-divider)]'}"
                    style:background-color={preset.color}
                  >
                    {#if selected}
                      <WaveIcon name="check" size={13} class={onAccentFor(preset.color) === '#000000' ? 'text-black' : 'text-white'} />
                    {/if}
                  </button>
                {/each}
                <button
                  type="button"
                  onclick={() => (colorPickerMode = 'base')}
                  use:waveTooltip={'Pick any base colour'}
                  aria-label="Pick any base colour"
                  class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border-2 transition-transform hover:scale-105 {!isPresetBase
                    ? 'border-[var(--lw-accent)]'
                    : 'border-[var(--lw-divider)]'}"
                  style:background-color={!isPresetBase ? settingsStore.customThemeBase : 'var(--lw-surface-overlay)'}
                >
                  {#if !isPresetBase}
                    <WaveIcon name="check" size={14} class={onAccentFor(settingsStore.customThemeBase) === '#000000' ? 'text-black' : 'text-white'} />
                  {:else}
                    <WaveIcon name="palette" size={14} class="text-[var(--lw-text-secondary)]" />
                  {/if}
                </button>
              </div>

              <!-- Miniature generated theme preview (matches _ThemePreview in settings_page.dart) -->
              <div
                class="flex h-[132px] flex-col overflow-hidden rounded-[12px] border"
                style:background-color={customPreviewSurfaces.background}
                style:border-color={customPreviewSurfaces.outlineSoft}
              >
                <div class="flex min-h-0 flex-1">
                  <div
                    class="flex w-9 flex-col items-center gap-3 pt-2.5 border-r"
                    style:background-color={customPreviewSurfaces.railTranslucent}
                    style:border-color={customPreviewSurfaces.outlineSoft}
                  >
                    <span style:color="var(--lw-accent)"><WaveIcon name="home" size={14} /></span>
                    <span style:color={customPreviewLight ? WaveColors.lightTextSecondary : WaveColors.textSecondary}><WaveIcon name="search" size={14} /></span>
                    <span style:color={customPreviewLight ? WaveColors.lightTextSecondary : WaveColors.textSecondary}><WaveIcon name="heart" size={14} /></span>
                  </div>
                  <div class="flex flex-1 p-2.5">
                    <div
                      class="flex flex-1 flex-col justify-between rounded-[10px] border p-2.5"
                      style:background-color={customPreviewSurfaces.surface}
                      style:border-color={customPreviewSurfaces.outlineSoft}
                    >
                      <span
                        class="text-[13px] font-semibold"
                        style:color={customPreviewLight ? WaveColors.lightTextPrimary : WaveColors.textPrimary}
                      >
                        Now playing
                      </span>
                      <div class="grid grid-cols-2 gap-2">
                        <div
                          class="flex h-7 items-center rounded-[10px] px-2.5 text-[12px]"
                          style:background-color={customPreviewSurfaces.surfaceRaised}
                          style:color={customPreviewLight ? WaveColors.lightTextSecondary : WaveColors.textSecondary}
                        >
                          Raised
                        </div>
                        <div
                          class="flex h-7 items-center rounded-[10px] px-2.5 text-[12px]"
                          style:background-color={customPreviewSurfaces.surfaceOverlay}
                          style:color={customPreviewLight ? WaveColors.lightTextSecondary : WaveColors.textSecondary}
                        >
                          Hover
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
                <div
                  class="flex h-[26px] items-center gap-2 border-t px-2.5"
                  style:background-color={customPreviewSurfaces.dockTranslucent}
                  style:border-color={customPreviewSurfaces.outlineSoft}
                >
                  <span class="h-2.5 w-2.5 rounded-full bg-[var(--lw-accent)]"></span>
                  <span
                    class="h-[3px] flex-1 rounded-full opacity-35"
                    style:background-color={customPreviewLight ? WaveColors.lightTextSecondary : WaveColors.textSecondary}
                  ></span>
                </div>
              </div>

              <p class="text-[12px] text-[var(--lw-text-tertiary)]">
                Dark colours give light text, light colours give dark text. Very bright or vivid picks are toned down so text stays readable.
              </p>
            </div>
          {/if}

          {#if settingsStore.themeMode === 'artwork'}
            <p class="text-[12px] text-[var(--lw-text-tertiary)]">
              Surfaces take their colour from the song that is playing and blend as tracks change. Midnight while nothing plays.
            </p>
          {/if}

          <!-- Accent source -->
          <div class="space-y-1.5 pt-1">
            <div class="text-[13.5px] font-semibold">Accent source</div>
            <Select
              label="Accent source"
              value={settingsStore.accentSource}
              options={[
                { value: 'custom', label: 'Custom colour' },
                { value: 'lastwave', label: 'LastWave neutral' },
                { value: 'system', label: 'System accent' },
                { value: 'artwork', label: 'Artwork tint' },
              ]}
              onchange={(v) => {
                settingsStore.accentSource = v as AccentSource;
                settingsStore.saveStringPref('lw_accent_source', v);
                settingsStore.saveStringPref('lw_accent_mode', v);
                syncThemeDocument();
              }}
            />

            <!-- _AccentPreview -->
            <div class="flex items-center gap-2 pt-1">
              <span
                class="h-3.5 w-3.5 shrink-0 rounded-[4px] border border-[var(--lw-divider)]"
                style:background-color={accentRes.color}
              ></span>
              <span class="text-[12px] text-[var(--lw-text-secondary)]">{accentCaption}</span>
            </div>
          </div>

          <!-- Accent colour swatches -->
          <div class="space-y-1.5 pt-0.5">
            <div class="text-[13.5px] font-semibold">Accent colour</div>
            <div class="flex flex-wrap gap-2">
              {#each ACCENT_PRESETS as c (c)}
                {@const selected = c.toLowerCase() === settingsStore.accentColor.toLowerCase()}
                <button
                  type="button"
                  onclick={() => {
                    settingsStore.setAccentColor(c);
                    syncThemeDocument();
                  }}
                  aria-label={`Accent ${c}`}
                  class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border-2 transition-transform hover:scale-105 {selected
                    ? 'border-[var(--lw-accent)]'
                    : 'border-[var(--lw-divider)]'}"
                  style:background-color={c}
                >
                  {#if selected}
                    <WaveIcon name="check" size={13} class={onAccentFor(c) === '#000000' ? 'text-black' : 'text-white'} />
                  {/if}
                </button>
              {/each}
              <button
                type="button"
                onclick={() => (colorPickerMode = 'accent')}
                use:waveTooltip={'Pick any accent colour'}
                aria-label="Pick any accent colour"
                class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border-2 transition-transform hover:scale-105 {!isPresetAccent
                  ? 'border-[var(--lw-accent)]'
                  : 'border-[var(--lw-divider)]'}"
                style:background-color={!isPresetAccent ? settingsStore.accentColor : 'var(--lw-surface-overlay)'}
              >
                {#if !isPresetAccent}
                  <WaveIcon name="check" size={14} class={onAccentFor(settingsStore.accentColor) === '#000000' ? 'text-black' : 'text-white'} />
                {:else}
                  <WaveIcon name="palette" size={14} class="text-[var(--lw-text-secondary)]" />
                {/if}
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- 2. Haze material -->
      <div id="settings-group-appearance.haze" class="lw-settings-group" data-spotlight={highlightId === 'appearance.haze'}>
        {@render groupHeader('Haze material', 'Automatic uses Haze L1–L3 as designed · Solid disables blur')}
        <div class="lw-settings-group-body space-y-3">
          <Select
            label="Haze material"
            value={settingsStore.hazeMaterial}
            options={[
              { value: 'automatic', label: 'Automatic' },
              { value: 'haze', label: 'Haze' },
              { value: 'solid', label: 'Solid' },
            ]}
            onchange={(v) => {
              settingsStore.hazeMaterial = v as HazeMaterial;
              settingsStore.saveStringPref('lw_haze_material', v);
              syncThemeDocument();
            }}
          />

          <div>
            <div class="mb-1.5 text-[13.5px] font-semibold">Haze intensity</div>
            <div class="flex flex-wrap gap-1.5">
              {#each ['low', 'medium', 'high'] as opt (opt)}
                <Buttons
                  variant="toggle"
                  size="sm"
                  active={settingsStore.hazeIntensity === opt}
                  onclick={() => {
                    settingsStore.hazeIntensity = opt as HazeIntensity;
                    settingsStore.saveStringPref('lw_haze_intensity', opt);
                    syncThemeDocument();
                  }}
                >
                  {opt[0].toUpperCase()}{opt.slice(1)}
                </Buttons>
              {/each}
            </div>
          </div>
        </div>
      </div>

      <!-- 3. Now Playing style -->
      <div id="settings-group-appearance.now_playing_style" class="lw-settings-group" data-spotlight={highlightId === 'appearance.now_playing_style'}>
        {@render groupHeader('Now Playing style', 'Layout and presentation for the player screen')}
        <div class="lw-settings-group-body">
          <NowPlayingStylePicker />
        </div>
      </div>

      <!-- 4. Auto-hide Now Playing controls -->
      <div id="settings-group-appearance.now_playing_autohide" class="lw-settings-group" data-spotlight={highlightId === 'appearance.now_playing_autohide'}>
        {@render groupHeader('Auto-hide Now Playing controls', 'Fade out the top buttons after mouse inactivity')}
        <div class="lw-settings-group-body">
          <div class="grid grid-cols-4 gap-2">
            {#each [3, 4, 5, 0] as sec (sec)}
              <Buttons
                variant="toggle"
                active={settingsStore.nowPlayingAutoHideSec === sec}
                class="w-full"
                onclick={() => {
                  settingsStore.nowPlayingAutoHideSec = sec;
                  settingsStore.saveIntPref('lw_now_playing_autohide_sec', sec);
                }}
              >
                {sec === 0 ? 'Off' : `${sec} sec`}
              </Buttons>
            {/each}
          </div>
        </div>
      </div>

      <!-- 5. Now Playing ambient -->
      <div id="settings-group-appearance.ambient" class="lw-settings-group" data-spotlight={highlightId === 'appearance.ambient'}>
        {@render groupHeader('Now Playing ambient', 'Background behind the cover and lyrics')}
        <div class="lw-settings-group-body space-y-2">
          <div class="grid grid-cols-3 gap-2">
            {#each [
              { id: 'apple', label: 'Apple Music', active: settingsStore.ambientStyle === 'apple' || settingsStore.ambientStyle === 'aurora' || settingsStore.ambientStyle === 'mesh' },
              { id: 'soft', label: 'Soft', active: settingsStore.ambientStyle === 'soft' || settingsStore.ambientStyle === 'wash' },
              { id: 'off', label: 'Off', active: settingsStore.ambientStyle === 'off' },
            ] as amb (amb.id)}
              <Buttons
                variant="toggle"
                active={amb.active}
                class="w-full"
                onclick={() => {
                  settingsStore.ambientStyle = amb.id as AmbientStyle;
                  settingsStore.saveStringPref('lw_ambient_style', amb.id);
                }}
              >
                {amb.label}
              </Buttons>
            {/each}
          </div>
          <p class="text-[12px] text-[var(--lw-text-tertiary)]">
            {settingsStore.ambientStyle === 'off'
              ? 'No artwork wash behind Now Playing.'
              : settingsStore.ambientStyle === 'soft' || settingsStore.ambientStyle === 'wash'
                ? 'A calm tone from the artwork that follows your theme (light in Pearl).'
                : 'Vivid colours from the artwork on a dark stage with white text. Looks the same in every theme.'}
          </p>
        </div>
      </div>

      <!-- 6. Font -->
      <div id="settings-group-appearance.font" class="lw-settings-group" data-spotlight={highlightId === 'appearance.font'}>
        {@render groupHeader('Font', 'The typeface used across the app')}
        <div class="lw-settings-group-body">
          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">App font</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                {isWebFont(settingsStore.appFont)
                  ? 'Downloaded once, then works offline.'
                  : 'Installed with your system.'}
              </div>
            </div>
            <div class="w-[220px] shrink-0">
              <Select
                expanded
                label="App font"
                value={APP_FONTS.some((f) => f.id === settingsStore.appFont) ? settingsStore.appFont : ''}
                options={APP_FONTS.map((f) => ({
                  value: f.id,
                  label: f.label,
                  fontFamily: fontFamilyFor(f.id) || undefined,
                }))}
                onchange={(v) => {
                  settingsStore.appFont = v;
                  settingsStore.saveStringPref('lw_app_font', v);
                  void ensureWebFontLoaded(v);
                  syncThemeDocument();
                }}
              />
            </div>
          </div>
        </div>
      </div>

      <!-- 7. Interface scale -->
      <div id="settings-group-appearance.scale" class="lw-settings-group" data-spotlight={highlightId === 'appearance.scale'}>
        {@render groupHeader(
          'Interface scale',
          `Adjust app zoom for display resolution or preference (${shortcutMod} +/-)`
        )}
        <div class="lw-settings-group-body">
          <div class="settings-behaviour-row">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Zoom level</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Shortcuts: {shortcutMod} + to zoom in, {shortcutMod} - to zoom out, {shortcutMod} 0 to reset.
              </div>
            </div>
            <div class="w-[220px] shrink-0">
              <Select
                expanded
                label="Zoom level"
                value={String(settingsStore.uiScale)}
                options={UI_SCALE_OPTIONS.map((v) => ({
                  value: String(v),
                  label: `${Math.round(v * 100)}%`,
                  hint: Math.abs(v - 1.0) < 0.001 ? 'Default' : `${Math.round(v * 100)}%`,
                }))}
                onchange={(v) => {
                  const n = Number(v);
                  setUiScale(n);
                  settingsStore.saveDoublePref('lw_ui_scale', n);
                }}
              />
            </div>
          </div>
        </div>
      </div>

      <!-- 8. Surfaces -->
      <div id="settings-group-appearance.surfaces" class="lw-settings-group mt-2" data-spotlight={highlightId === 'appearance.surfaces'}>
        {@render groupHeader('Surfaces', 'Contrast and artwork tinting')}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">AMOLED black</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Pure-black surfaces · Midnight only
              </div>
            </div>
            <Switch
              label="AMOLED black"
              disabled={!isMidnight}
              checked={settingsStore.amoled}
              onchange={(v) => {
                settingsStore.setAmoled(v);
                syncThemeDocument();
              }}
            />
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Dynamic artwork theme</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Tint Now Playing from album art
              </div>
            </div>
            <Switch
              label="Dynamic artwork theme"
              checked={settingsStore.dynamicNowPlaying}
              onchange={(v) => {
                settingsStore.dynamicNowPlaying = v;
                settingsStore.saveBoolPref('lw_dynamic_now_playing', v);
              }}
            />
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Reduce motion</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Minimize entrances, spring physics, and animated transitions
              </div>
            </div>
            <Switch
              label="Reduce motion"
              checked={settingsStore.reduceMotion}
              onchange={(v) => {
                settingsStore.reduceMotion = v;
                settingsStore.saveBoolPref('lw_reduce_motion', v);
              }}
            />
          </div>
        </div>
      </div>
    </div>

  {:else if activeSection === 'lastfm'}
    <div class="space-y-3">
      <!-- Scrobbler rules -->
      <div id="settings-group-lastfm.scrobbling" class="lw-settings-group" data-spotlight={highlightId === 'lastfm.scrobbling'}>
        {@render groupHeader('Scrobbler rules', 'Thresholds apply across all connected scrobblers')}
        <div class="lw-settings-group-body space-y-3">
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Enable scrobbling</div>
              <div class="text-[12px] text-[var(--lw-text-secondary)]">
                Send played tracks to connected scrobbler accounts
              </div>
            </div>
            <Switch
              label="Enable scrobbling"
              checked={settingsStore.scrobbleEnabled}
              onchange={(v) => {
                settingsStore.scrobbleEnabled = v;
                settingsStore.saveBoolPref('lw_scrobbler_enabled', v);
              }}
            />
          </div>

          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="text-[13.5px] font-semibold">Now playing updates</div>
            </div>
            <Switch
              label="Now playing updates"
              checked={settingsStore.scrobbleNowPlaying}
              onchange={(v) => {
                settingsStore.scrobbleNowPlaying = v;
                settingsStore.saveBoolPref('lw_submit_now_playing', v);
              }}
            />
          </div>

          <div class="flex items-center justify-between gap-4 pt-1">
            <span class="min-w-0 flex-1 text-[13.5px] font-semibold lw-tabular">
              Scrobble at {settingsStore.scrobblePercent}% of track
            </span>
            <div class="w-[180px] shrink-0">
              <WaveSlider
                min={25}
                max={90}
                step={1}
                label="Scrobble threshold"
                valuetext={`${settingsStore.scrobblePercent}%`}
                value={settingsStore.scrobblePercent}
                oninput={(v) => (settingsStore.scrobblePercent = Math.round(v))}
                onchange={(v) => settingsStore.saveIntPref('lw_scrobble_percent', Math.round(v))}
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Scrobbler accounts -->
      <div id="settings-group-lastfm.connection" class="lw-settings-group" data-spotlight={highlightId === 'lastfm.connection'}>
        {@render groupHeader('Scrobbler accounts', 'Connect Last.fm, ListenBrainz, or Libre.fm to scrobble and sync stats')}
        <div class="lw-settings-group-body">
          <ScrobblerSettings onOpenGuide={() => (keyGuideOpen = true)} />
        </div>
      </div>
    </div>

  {:else if activeSection === 'sources'}
    {@const activeAddonsCount = settingsStore.addonsEnabled
      ? settingsStore.addonUrls.filter((u) => !settingsStore.disabledAddonUrls.includes(u)).length
      : 0}
    <div id="settings-group-sources.addons" class="lw-settings-group" data-spotlight={highlightId === 'sources.addons'}>
      {@render groupHeader(
        'Addon sources',
        settingsStore.addonUrls.length === 0
          ? 'No addon configured — using the built-in lossless backend'
          : !settingsStore.addonsEnabled
            ? 'All addons turned off — using the built-in lossless backend'
            : activeAddonsCount === 0
              ? 'All addons paused individually — using the built-in lossless backend'
              : 'Addons serve the lossless tier instead of the built-in backend'
      )}
      <div class="lw-settings-group-body space-y-3">
        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Enable addons</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Turn off to pause all configured addons without removing their URLs.
            </div>
          </div>
          <Switch
            label="Enable addons"
            checked={settingsStore.addonsEnabled}
            onchange={(v) => {
              settingsStore.addonsEnabled = v;
              settingsStore.saveBoolPref('lw_addons_enabled', v);
            }}
          />
        </div>

        {#if settingsStore.addonUrls.length > 0}
          <div class="space-y-2 pt-1">
            {#each settingsStore.addonUrls as url (url)}
              {@const enabled = !settingsStore.disabledAddonUrls.includes(url)}
              {@const effectiveOn = settingsStore.addonsEnabled && enabled}
              {@const host = (() => {
                try {
                  return new URL(url).host;
                } catch {
                  return url;
                }
              })()}
              <div class="flex items-center gap-2.5 transition-opacity {effectiveOn ? 'opacity-100' : 'opacity-55'}">
                <WaveIcon name="cloud" size={18} class="shrink-0 text-[var(--lw-text-secondary)]" />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13.5px] font-semibold">{host}</div>
                  <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">
                    {effectiveOn ? host : `${host} · off`}
                  </div>
                </div>
                <Switch
                  label={`Toggle addon ${host}`}
                  disabled={!settingsStore.addonsEnabled}
                  checked={enabled}
                  onchange={(v) => {
                    const next = v
                      ? settingsStore.disabledAddonUrls.filter((u) => u !== url)
                      : [...settingsStore.disabledAddonUrls, url];
                    persistDisabledAddons(next);
                  }}
                />
                <Buttons
                  variant="secondary"
                  size="sm"
                  onclick={() => {
                    persistAddonList(settingsStore.addonUrls.filter((u) => u !== url));
                    persistDisabledAddons(settingsStore.disabledAddonUrls.filter((u) => u !== url));
                  }}
                >
                  Remove
                </Buttons>
              </div>
            {/each}
          </div>
        {/if}

        <div>
          <Buttons
            variant="primary"
            size="sm"
            onclick={() => {
              addonDraft = '';
              addonError = '';
              addonDialog?.showModal();
            }}
          >
            Add addon
          </Buttons>
        </div>

        <p class="text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
          Paste the full addon URL from your dashboard. Anyone holding it plays on your quota — keep the dashboard backup safe.
        </p>
      </div>
    </div>

  {:else if activeSection === 'experimental'}
    <div id="settings-group-experimental.main" class="lw-settings-group" data-spotlight={highlightId === 'experimental.main'}>
      {@render groupHeader('Experimental', 'Opt-in desktop prototypes')}
      <div class="lw-settings-group-body space-y-3">
        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Extra translucency</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Stronger Haze on panels (may cost FPS)
            </div>
          </div>
          <Switch
            label="Extra translucency"
            checked={settingsStore.liquidGlass}
            onchange={(v) => {
              settingsStore.liquidGlass = v;
              settingsStore.saveBoolPref('lw_liquid_glass', v);
              syncThemeDocument();
            }}
          />
        </div>

        <div class="flex items-center justify-between gap-3 py-1">
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold">Wavy seekbar</div>
            <div class="text-[12px] text-[var(--lw-text-secondary)]">
              Experimental timeline treatment
            </div>
          </div>
          <Switch
            label="Wavy seekbar"
            checked={settingsStore.wavySeekbar}
            onchange={(v) => {
              settingsStore.wavySeekbar = v;
              settingsStore.saveBoolPref('lw_wavy_seekbar', v);
            }}
          />
        </div>
      </div>
    </div>

  {:else}
    <!-- About -->
    <div id="settings-group-about.main" class="lw-settings-group" data-spotlight={highlightId === 'about.main'}>
      {@render groupHeader('About LastWave', 'Build and project links')}
      <div class="lw-settings-group-body space-y-3.5">
        <div class="flex flex-wrap items-center gap-3">
          <span class="text-[13.5px] font-semibold">LastWave Desktop · v2.6.0</span>
          {#if checkingUpdates}
            <Buttons variant="secondary" size="sm" disabled>
              <WaveIcon name="refresh" size={14} class="animate-spin" />
              Checking…
            </Buttons>
          {:else if availableVersion}
            <Buttons variant="primary" size="sm" onclick={() => (windowState.updateDialogOpen = true)}>
              Update available: v{availableVersion}
            </Buttons>
          {:else if upToDate}
            <Buttons variant="secondary" size="sm" onclick={() => void checkForUpdates()}>
              <WaveIcon name="check" size={14} class="text-[var(--lw-success)]" />
              Up to date (check again)
            </Buttons>
          {:else}
            <Buttons variant="secondary" size="sm" onclick={() => void checkForUpdates()}>
              <WaveIcon name="refresh" size={14} />
              Check for updates
            </Buttons>
          {/if}
        </div>

        <div class="flex flex-wrap gap-2">
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => openExternal('https://github.com/Clash-Projects/LastWave-Desktop')}
          >
            <WaveIcon name="external" size={14} />
            GitHub repository
          </Buttons>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => openExternal('https://github.com/Clash-Projects/LastWave-Desktop/issues')}
          >
            <WaveIcon name="error" size={14} />
            Report an issue
          </Buttons>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => openExternal('https://github.com/Clash-Projects/LastWave-Desktop')}
          >
            <WaveIcon name="star" size={14} />
            Star / support the project
          </Buttons>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => openExternal('https://t.me/clashprojects')}
          >
            <BrandIcon name="telegram" size={14} />
            Telegram community
          </Buttons>
          <Buttons
            variant="secondary"
            size="sm"
            onclick={() => openExternal('https://discord.com/invite/TMCEPSUNk2')}
          >
            <BrandIcon name="discord" size={14} />
            Discord server
          </Buttons>
        </div>

        <div class="flex flex-wrap gap-x-3 gap-y-1 pt-1 text-[11.5px] text-[var(--lw-text-tertiary)]">
          <span>Lyrics renderer · am-lyrics 1.9.2 by binimum</span>
          <a class="underline hover:text-[var(--lw-accent)]" href="/licenses/am-lyrics.txt" target="_blank" rel="noopener noreferrer">MPL-2.0 license</a>
        </div>
      </div>
    </div>
  {/if}
{/snippet}

<div bind:clientWidth={pageWidth} data-lw="settings-page" class="flex h-full flex-col">
  <!-- Top search box (matches SettingsSearchBox in settings_page.dart) -->
  <div class="relative z-20 px-6 pt-3.5">
    <div class="relative max-w-[640px]">
      <Input
        type="search"
        bind:value={query}
        prefixIcon="search"
        placeholder="Search settings"
        onkeydown={(e) => {
          if (e.key === 'Enter' && searchHits.length > 0) {
            e.preventDefault();
            jumpToEntry(searchHits[0]);
          } else if (e.key === 'Escape' && query) {
            query = '';
          }
        }}
      />

      {#if query.trim()}
        <div class="lw-flyout-surface absolute top-11 left-0 z-40 w-full">
          {#if searchHits.length === 0}
            <div class="px-3 py-2 text-[12px] text-[var(--lw-text-secondary)]">
              No settings match "{query.trim()}".
            </div>
          {:else}
            {#each searchHits as hit (hit.id)}
              <button
                type="button"
                onclick={() => jumpToEntry(hit)}
                class="flex w-full cursor-pointer flex-col rounded-[10px] px-3 py-1.5 text-left hover:bg-[var(--lw-wash-hover)]"
              >
                <span class="text-[13px] font-semibold text-[var(--lw-text-primary)]">{hit.title}</span>
                <span class="truncate text-[11.5px] text-[var(--lw-text-secondary)]">
                  {sectionDisplayLabel(hit.section)} · {hit.subtitle}
                </span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  </div>

  {#if narrow}
    <!-- Narrow single-column layout (< 900px, matches settings_page.dart lines 263-296) -->
    <div use:waveSmoothScroll class="lw-scroll min-h-0 flex-1 px-4 pt-4 pb-10">
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-accent)] uppercase">SYSTEM</div>
      <h1 class="text-[22px] font-bold tracking-[-0.4px]">Settings</h1>

      <div class="mt-2.5 flex flex-wrap gap-1.5">
        {#each SECTIONS as sec (sec.id)}
          <Buttons
            variant="toggle"
            size="sm"
            active={activeSection === sec.id}
            onclick={() => (activeSection = sec.id)}
          >
            {sec.label}
          </Buttons>
        {/each}
      </div>

      <div class="mt-3.5 max-w-[640px]">
        {#key activeSection}
          <div class="lw-route-enter">
            {@render sectionContent()}
          </div>
        {/key}
      </div>
    </div>
  {:else}
    <!-- Wide two-pane layout (>= 900px: 220px rail + 1px divider + 640px content, matches settings_page.dart lines 297-355) -->
    <div class="flex min-h-0 flex-1 items-stretch">
      <aside
        use:waveSmoothScroll
        class="lw-scroll w-[220px] shrink-0 pt-5 pr-3 pb-8 pl-6"
      >
        <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-accent)] uppercase">SYSTEM</div>
        <h1 class="mb-3 text-[22px] font-bold tracking-[-0.4px]">Settings</h1>

        <nav class="space-y-0.5">
          {#each SECTIONS as sec (sec.id)}
            {@const active = activeSection === sec.id}
            <button
              type="button"
              onclick={() => (activeSection = sec.id)}
              class="relative flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-[10px] px-3 text-[13px] transition-colors duration-[120ms] ease-[var(--ease-native)] {active
                ? 'bg-[var(--lw-wash-pressed)] font-semibold text-[var(--lw-text-primary)]'
                : 'text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)] hover:text-[var(--lw-text-primary)]'}"
            >
              {#if active}
                <span
                  aria-hidden="true"
                  class="absolute left-1 h-4 w-[3px] rounded-full bg-[var(--lw-accent)]"
                ></span>
              {/if}
              <WaveIcon name={sec.icon} size={15} />
              <span class="truncate">{sec.label}</span>
            </button>
          {/each}
        </nav>
      </aside>

      <!-- 1px vertical divider matching Container(width: 1, color: waveDivider(context)) -->
      <div class="w-px shrink-0 bg-[var(--lw-divider)]"></div>

      <div use:waveSmoothScroll class="lw-scroll min-w-0 flex-1 px-6 pt-5 pb-12">
        <div class="max-w-[640px]">
          {#key activeSection}
            <div class="lw-route-enter">
              {@render sectionContent()}
            </div>
          {/key}
        </div>
      </div>
    </div>
  {/if}
</div>

<ColorPickerDialog
  open={colorPickerMode !== null}
  title={colorPickerMode === 'base' ? 'Theme base colour' : 'Accent colour'}
  initialColor={colorPickerMode === 'base' ? settingsStore.customThemeBase : settingsStore.accentColor}
  onClose={() => (colorPickerMode = null)}
  onApply={(hex) => {
    if (colorPickerMode === 'base') {
      setCustomBase(hex);
    } else {
      settingsStore.setAccentColor(hex);
      syncThemeDocument();
    }
  }}
/>

<!-- Add addon dialog (matches _addAddonDialog in settings_page.dart) -->
<dialog
  bind:this={addonDialog}
  closedby="any"
  use:dismissDialogOnBackdrop={() => addonDialog?.close()}
  aria-label="Add addon"
  class="w-[min(440px,calc(100vw-32px))] p-6 text-[var(--lw-text-primary)]"
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submitNewAddon();
    }}
    class="space-y-4"
  >
    <h2 class="text-[18px] font-semibold">Add addon</h2>
    <Input
      bind:value={addonDraft}
      autofocus
      placeholder="https://…/a/<token>/ or https://…"
      oninput={() => (addonError = '')}
    />
    {#if addonError}
      <p class="text-[12px] text-[var(--lw-danger)]">{addonError}</p>
    {/if}
    <div class="flex justify-end gap-2 pt-1">
      <Buttons variant="secondary" size="sm" onclick={() => addonDialog?.close()}>Cancel</Buttons>
      <Buttons type="submit" variant="primary" size="sm" disabled={!addonDraft.trim()}>Add</Buttons>
    </div>
  </form>
</dialog>

<LastfmKeyGuide open={keyGuideOpen} onClose={() => (keyGuideOpen = false)} />

<style>
  .settings-behaviour-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-block: 2px;
  }

  @media (max-width: 560px) {
    .settings-behaviour-row {
      flex-direction: column;
      align-items: stretch;
    }
    .settings-behaviour-row > div:last-child {
      width: 100%;
    }
  }
</style>
