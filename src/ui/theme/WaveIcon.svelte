<script module lang="ts">
  export type WaveIconName =
    | 'activity'
    | 'addToQueue'
    | 'album'
    | 'alert'
    | 'arrowLeft'
    | 'arrowRight'
    | 'artist'
    | 'blocked'
    | 'bookmark'
    | 'brush'
    | 'bug'
    | 'check'
    | 'checkList'
    | 'chevronDown'
    | 'chevronLeft'
    | 'chevronRight'
    | 'chevronUp'
    | 'chromeMaximize'
    | 'chromeMinimize'
    | 'chromeRestore'
    | 'clear'
    | 'clock'
    | 'close'
    | 'cloud'
    | 'command'
    | 'compass'
    | 'contact'
    | 'copy'
    | 'down'
    | 'download'
    | 'drive'
    | 'edit'
    | 'eq'
    | 'error'
    | 'exitFullscreen'
    | 'expand'
    | 'external'
    | 'folder'
    | 'folderOpen'
    | 'fontSize'
    | 'gauge'
    | 'genres'
    | 'globe'
    | 'gripper'
    | 'gripperBar'
    | 'headphones'
    | 'heart'
    | 'history'
    | 'home'
    | 'info'
    | 'jam'
    | 'layers'
    | 'library'
    | 'link'
    | 'list'
    | 'localeLanguage'
    | 'lock'
    | 'lyrics'
    | 'maximize'
    | 'miniContract'
    | 'miniPlayer'
    | 'minimize'
    | 'mixes'
    | 'moon'
    | 'more'
    | 'music'
    | 'next'
    | 'palette'
    | 'panelLeft'
    | 'pause'
    | 'photo'
    | 'pin'
    | 'pinned'
    | 'play'
    | 'playlists'
    | 'plugin'
    | 'plus'
    | 'podcast'
    | 'prev'
    | 'queue'
    | 'radio'
    | 'readingMode'
    | 'refresh'
    | 'remove'
    | 'repeat'
    | 'repeatOne'
    | 'reset'
    | 'search'
    | 'settings'
    | 'share'
    | 'shuffle'
    | 'sort'
    | 'sortDown'
    | 'sortUp'
    | 'sparkles'
    | 'speaker'
    | 'star'
    | 'stats'
    | 'store'
    | 'streamPath'
    | 'sun'
    | 'sync'
    | 'terminal'
    | 'trash'
    | 'unlock'
    | 'unpin'
    | 'upload'
    | 'video'
    | 'visualizer'
    | 'volume'
    | 'volumeHigh'
    | 'volumeLow'
    | 'volumeMute'
    | 'wand';

  type IconFont = 'segoe' | 'fluent';

  interface GlyphSpec {
    font: IconFont;
    code: number;
    filledCode?: number;
  }

  /**
   * Exact 1:1 codepoints matching `WaveIcons` (`lib/ui/theme/wave_icons.dart`),
   * `WindowsIcons` (`SegoeIcons.ttf`), and `FluentIcons` (`FluentIcons.ttf`).
   */
  const GLYPHS: Record<WaveIconName, GlyphSpec> = {
    // Navigation (WindowsIcons / SegoeIcons + FluentIcons)
    home: { font: 'segoe', code: 0xe80f }, // WindowsIcons.home
    globe: { font: 'segoe', code: 0xe774 }, // WindowsIcons.globe
    stats: { font: 'fluent', code: 0xe9ec }, // FluentIcons.bar_chart_vertical
    search: { font: 'segoe', code: 0xe721 }, // WindowsIcons.search
    library: { font: 'segoe', code: 0xe8f1 }, // WindowsIcons.library
    heart: { font: 'segoe', code: 0xeb51, filledCode: 0xeb52 }, // WindowsIcons.heart / heart_fill
    album: { font: 'segoe', code: 0xe93c }, // WindowsIcons.music_album
    artist: { font: 'segoe', code: 0xe720 }, // WindowsIcons.microphone
    contact: { font: 'fluent', code: 0xe77b }, // FluentIcons.contact
    playlists: { font: 'segoe', code: 0xe90b }, // WindowsIcons.music_info
    download: { font: 'segoe', code: 0xe896 }, // WindowsIcons.download
    jam: { font: 'segoe', code: 0xe716 }, // WindowsIcons.people
    settings: { font: 'segoe', code: 0xe713 }, // WindowsIcons.settings
    history: { font: 'segoe', code: 0xe81c }, // WindowsIcons.history
    mixes: { font: 'segoe', code: 0xe8d6 }, // WindowsIcons.audio
    podcast: { font: 'fluent', code: 0xe95b }, // FluentIcons.headset
    headphones: { font: 'fluent', code: 0xe95b }, // FluentIcons.headset
    genres: { font: 'segoe', code: 0xe8ec }, // WindowsIcons.tag
    store: { font: 'segoe', code: 0xe719 }, // WindowsIcons.shop
    plugin: { font: 'segoe', code: 0xea86 }, // WindowsIcons.puzzle

    // Transport
    play: { font: 'fluent', code: 0xe768 }, // FluentIcons.play
    pause: { font: 'fluent', code: 0xe769 }, // FluentIcons.pause
    prev: { font: 'fluent', code: 0xe892 }, // FluentIcons.previous
    next: { font: 'fluent', code: 0xe893 }, // FluentIcons.next
    shuffle: { font: 'segoe', code: 0xe8b1 }, // WindowsIcons.shuffle
    repeat: { font: 'fluent', code: 0xe8ee }, // FluentIcons.repeat_all
    repeatOne: { font: 'fluent', code: 0xe8ed }, // FluentIcons.repeat_one

    // Content & Audio
    music: { font: 'fluent', code: 0xec4f }, // FluentIcons.music_note
    lyrics: { font: 'fluent', code: 0xec4f }, // FluentIcons.music_note (WaveIcons.lyrics)
    queue: { font: 'segoe', code: 0xe8fd }, // WindowsIcons.bulleted_list (WaveIcons.queue)
    list: { font: 'fluent', code: 0xea37 }, // FluentIcons.list
    addToQueue: { font: 'fluent', code: 0xecc8 }, // FluentIcons.add_to (WaveIcons.addTo)
    radio: { font: 'fluent', code: 0xeccb }, // FluentIcons.radio_btn_on
    speaker: { font: 'fluent', code: 0xe7f5 }, // FluentIcons.speakers
    video: { font: 'fluent', code: 0xe714 }, // FluentIcons.video (WaveIcons.canvas)
    volume: { font: 'fluent', code: 0xe994 }, // FluentIcons.volume2 (WaveIcons.volume)
    volumeHigh: { font: 'fluent', code: 0xe994 }, // FluentIcons.volume2
    volumeLow: { font: 'fluent', code: 0xe993 }, // FluentIcons.volume1
    volumeMute: { font: 'fluent', code: 0xea85 }, // FluentIcons.volume_disabled
    eq: { font: 'fluent', code: 0xe9e9 }, // FluentIcons.equalizer
    visualizer: { font: 'segoe', code: 0xebc5 }, // WindowsIcons.volume_bars (WaveIcons.visualizer)
    streamPath: { font: 'fluent', code: 0xef90 }, // FluentIcons.flow (WaveIcons.streamPath)
    gauge: { font: 'fluent', code: 0xec4a }, // FluentIcons.speed_high (WaveIcons.gauge)
    activity: { font: 'fluent', code: 0xec4a }, // FluentIcons.speed_high

    // Window Chrome & Sizing
    panelLeft: { font: 'fluent', code: 0xe700 }, // FluentIcons.global_nav_button
    command: { font: 'fluent', code: 0xe756 }, // FluentIcons.command_prompt
    terminal: { font: 'fluent', code: 0xe756 }, // FluentIcons.command_prompt
    close: { font: 'fluent', code: 0xe8bb }, // FluentIcons.chrome_close
    clear: { font: 'fluent', code: 0xe894 }, // FluentIcons.clear
    reset: { font: 'fluent', code: 0xe423 }, // FluentIcons.reset
    chromeMinimize: { font: 'fluent', code: 0xe921 }, // FluentIcons.chrome_minimize
    chromeMaximize: { font: 'fluent', code: 0xe739 }, // FluentIcons.checkbox
    chromeRestore: { font: 'fluent', code: 0xe923 }, // FluentIcons.chrome_restore
    expand: { font: 'fluent', code: 0xe740 }, // FluentIcons.full_screen (WaveIcons.expand)
    maximize: { font: 'fluent', code: 0xe740 }, // FluentIcons.full_screen
    exitFullscreen: { font: 'fluent', code: 0xe73f }, // FluentIcons.back_to_window
    miniPlayer: { font: 'fluent', code: 0xe93a }, // FluentIcons.mini_expand (WaveIcons.miniPlayer)
    minimize: { font: 'fluent', code: 0xe93a }, // FluentIcons.mini_expand
    miniContract: { font: 'fluent', code: 0xe93b }, // FluentIcons.mini_contract

    // Directional & Sorting
    chevronLeft: { font: 'fluent', code: 0xe76b }, // FluentIcons.chevron_left
    chevronRight: { font: 'fluent', code: 0xe76c }, // FluentIcons.chevron_right
    chevronUp: { font: 'fluent', code: 0xe70e }, // FluentIcons.chevron_up
    chevronDown: { font: 'fluent', code: 0xe70d }, // FluentIcons.chevron_down
    arrowLeft: { font: 'fluent', code: 0xe72b }, // FluentIcons.back
    arrowRight: { font: 'fluent', code: 0xe72a }, // FluentIcons.forward
    down: { font: 'fluent', code: 0xe74b }, // FluentIcons.down
    sort: { font: 'fluent', code: 0xe8cb }, // FluentIcons.sort
    sortUp: { font: 'fluent', code: 0xee68 }, // FluentIcons.sort_up
    sortDown: { font: 'fluent', code: 0xee69 }, // FluentIcons.sort_down

    // Actions, Lyrics & Settings
    more: { font: 'fluent', code: 0xe712 }, // FluentIcons.more
    plus: { font: 'fluent', code: 0xe710 }, // FluentIcons.add
    remove: { font: 'fluent', code: 0xe738 }, // FluentIcons.remove
    check: { font: 'fluent', code: 0xe73e }, // FluentIcons.check_mark
    checkList: { font: 'fluent', code: 0xe9d5 }, // FluentIcons.check_list
    pin: { font: 'fluent', code: 0xe718, filledCode: 0xe840 }, // FluentIcons.pin / pinned
    pinned: { font: 'fluent', code: 0xe840 }, // FluentIcons.pinned
    unpin: { font: 'fluent', code: 0xe77a }, // FluentIcons.unpin
    edit: { font: 'fluent', code: 0xe70f }, // FluentIcons.edit
    trash: { font: 'fluent', code: 0xe74d }, // FluentIcons.delete
    clock: { font: 'fluent', code: 0xe917 }, // FluentIcons.clock
    refresh: { font: 'fluent', code: 0xe72c }, // FluentIcons.refresh
    sync: { font: 'fluent', code: 0xe895 }, // FluentIcons.sync
    info: { font: 'fluent', code: 0xe946 }, // FluentIcons.info
    alert: { font: 'fluent', code: 0xe7ba }, // FluentIcons.warning
    error: { font: 'fluent', code: 0xe783 }, // FluentIcons.error
    blocked: { font: 'fluent', code: 0xe733 }, // FluentIcons.blocked
    bookmark: { font: 'fluent', code: 0xe736 }, // FluentIcons.reading_mode
    readingMode: { font: 'fluent', code: 0xe736 }, // FluentIcons.reading_mode
    fontSize: { font: 'fluent', code: 0xe8e9 }, // FluentIcons.font_size
    localeLanguage: { font: 'fluent', code: 0xf2b7 }, // FluentIcons.locale_language
    brush: { font: 'fluent', code: 0xecff }, // FluentIcons.brush
    palette: { font: 'segoe', code: 0xe790 }, // WindowsIcons.color (WaveIcons.theme)
    link: { font: 'fluent', code: 0xe71b }, // FluentIcons.link
    share: { font: 'fluent', code: 0xe72d }, // FluentIcons.share
    copy: { font: 'fluent', code: 0xe8c8 }, // FluentIcons.copy
    external: { font: 'fluent', code: 0xe8a7 }, // FluentIcons.open_in_new_window
    folder: { font: 'fluent', code: 0xe8b7 }, // FluentIcons.folder
    folderOpen: { font: 'fluent', code: 0xed25 }, // FluentIcons.open_folder_horizontal
    drive: { font: 'fluent', code: 0xeda2 }, // FluentIcons.hard_drive
    layers: { font: 'segoe', code: 0xe8d6 }, // WindowsIcons.audio
    cloud: { font: 'fluent', code: 0xe753 }, // FluentIcons.cloud
    bug: { font: 'fluent', code: 0xebe8 }, // FluentIcons.bug
    photo: { font: 'fluent', code: 0xeb9f }, // FluentIcons.photo2
    star: { font: 'fluent', code: 0xe734, filledCode: 0xe735 }, // FluentIcons.favorite_star / _fill
    sparkles: { font: 'fluent', code: 0xe735 }, // FluentIcons.favorite_star_fill
    wand: { font: 'fluent', code: 0xe70f }, // FluentIcons.edit
    compass: { font: 'fluent', code: 0xe942 }, // FluentIcons.compass_n_w
    moon: { font: 'fluent', code: 0xe9c2 }, // FluentIcons.clear_night
    sun: { font: 'fluent', code: 0xe9bd }, // FluentIcons.sunny
    upload: { font: 'fluent', code: 0xe898 }, // FluentIcons.upload
    lock: { font: 'fluent', code: 0xe72e }, // FluentIcons.lock
    unlock: { font: 'fluent', code: 0xe785 }, // FluentIcons.unlock
    gripper: { font: 'fluent', code: 0xf772 }, // FluentIcons.gripper_dots_vertical
    gripperBar: { font: 'fluent', code: 0xe784 }, // FluentIcons.gripper_bar_vertical
  };

  // The outline font has no matching filled pause or skip glyphs.
  const SOLID_TRANSPORT: Partial<Record<WaveIconName, string>> = {
    play: 'M7 5.8a1 1 0 0 1 1.5-.87l10.7 6.2a1 1 0 0 1 0 1.74l-10.7 6.2A1 1 0 0 1 7 18.2z',
    pause: 'M7 5h2a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zm8 0h2a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1h-2a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z',
    prev: 'M4 6a1 1 0 0 1 2 0v12a1 1 0 0 1-2 0zm15 .55a1 1 0 0 0-1.5-.86l-9 5.45a1 1 0 0 0 0 1.72l9 5.45a1 1 0 0 0 1.5-.86z',
    next: 'M20 6a1 1 0 0 0-2 0v12a1 1 0 0 0 2 0zM5 6.55a1 1 0 0 1 1.5-.86l9 5.45a1 1 0 0 1 0 1.72l-9 5.45A1 1 0 0 1 5 17.45z',
  };
</script>

<script lang="ts">
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';

  interface Props {
    name: WaveIconName;
    size?: number;
    filled?: boolean;
    class?: string;
  }

  let { name, size = 18, filled = false, class: className = '' }: Props = $props();
  let customMaskUrl = $derived(uiSlots.iconMaskUrls[name]);
  let customSvg = $derived(uiSlots.iconOverrides[name]);
  let spec = $derived(GLYPHS[name] ?? GLYPHS.music);
  let codePoint = $derived(filled && spec.filledCode ? spec.filledCode : spec.code);
  let solidTransport = $derived(filled ? SOLID_TRANSPORT[name] : undefined);
</script>

{#if customMaskUrl}
  <span
    class="inline-block shrink-0 bg-current {className}"
    style:width="{size}px"
    style:height="{size}px"
    style:-webkit-mask-image={customMaskUrl}
    style:mask-image={customMaskUrl}
    style:-webkit-mask-repeat="no-repeat"
    style:mask-repeat="no-repeat"
    style:-webkit-mask-position="center"
    style:mask-position="center"
    style:-webkit-mask-size="contain"
    style:mask-size="contain"
    aria-hidden="true"
  ></span>
{:else if customSvg}
  <span
    class="inline-flex shrink-0 items-center justify-center [&>svg]:h-full [&>svg]:w-full {className}"
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
  >
    {@html customSvg}
  </span>
{:else if solidTransport}
  <svg class="shrink-0 {className}" width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d={solidTransport} />
  </svg>
{:else}
  <span
    class="lw-glyph {className}"
    style:font-family={spec.font === 'segoe' ? '"SegoeIcons"' : '"FluentIcons"'}
    style:font-size="{size}px"
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
  >{String.fromCodePoint(codePoint)}</span>
{/if}
