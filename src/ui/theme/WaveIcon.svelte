<script module lang="ts">
  export type WaveIconName =
    | 'activity'
    | 'alert'
    | 'arrowLeft'
    | 'arrowRight'
    | 'sort'
    | 'stats'
    | 'bookmark'
    | 'check'
    | 'chevronDown'
    | 'chevronLeft'
    | 'chevronRight'
    | 'chevronUp'
    | 'clock'
    | 'compass'
    | 'copy'
    | 'album'
    | 'download'
    | 'external'
    | 'folder'
    | 'folderOpen'
    | 'globe'
    | 'drive'
    | 'headphones'
    | 'heart'
    | 'home'
    | 'info'
    | 'layers'
    | 'library'
    | 'queue'
    | 'addToQueue'
    | 'maximize'
    | 'lyrics'
    | 'minimize'
    | 'moon'
    | 'more'
    | 'music'
    | 'palette'
    | 'pause'
    | 'pin'
    | 'play'
    | 'plus'
    | 'podcast'
    | 'plugin'
    | 'radio'
    | 'refresh'
    | 'repeat'
    | 'repeatOne'
    | 'search'
    | 'settings'
    | 'share'
    | 'store'
    | 'shuffle'
    | 'prev'
    | 'next'
    | 'eq'
    | 'sparkles'
    | 'speaker'
    | 'sun'
    | 'terminal'
    | 'trash'
    | 'upload'
    | 'artist'
    | 'jam'
    | 'volumeLow'
    | 'volumeHigh'
    | 'volumeMute'
    | 'wand'
    | 'close';
</script>

<script lang="ts">
  import {
    AlertCircle,
    ArrowLeft,
    ArrowRight,
    ArrowUpDown,
    AudioLines,
    BarChart3,
    Bookmark,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    Clock,
    Compass,
    Copy,
    Disc3,
    Download,
    Ellipsis,
    ExternalLink,
    Folder,
    FolderOpen,
    Globe,
    HardDrive,
    Headphones,
    Heart,
    Home,
    Info,
    Layers,
    Library,
    ListMusic,
    ListPlus,
    Maximize2,
    MicVocal,
    Moon,
    Music,
    Palette,
    Pause,
    PictureInPicture2,
    Pin,
    Play,
    Plus,
    Podcast,
    Puzzle,
    Radio,
    RefreshCw,
    Repeat,
    Repeat1,
    Search,
    Settings,
    Share2,
    ShoppingBag,
    Shuffle,
    SkipBack,
    SkipForward,
    SlidersVertical,
    Sparkles,
    Speaker,
    Sun,
    Terminal,
    Trash2,
    Upload,
    User,
    Users,
    Volume1,
    Volume2,
    VolumeX,
    WandSparkles,
    X,
  } from 'lucide-svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';

  interface Props {
    name: WaveIconName;
    size?: number;
    filled?: boolean;
    class?: string;
  }

  let { name, size = 18, filled = false, class: className = '' }: Props = $props();
  let customSvg = $derived(uiSlots.iconOverrides[name]);

  const ICONS: Record<WaveIconName, typeof Home> = {
    activity: AudioLines,
    alert: AlertCircle,
    arrowLeft: ArrowLeft,
    arrowRight: ArrowRight,
    sort: ArrowUpDown,
    stats: BarChart3,
    bookmark: Bookmark,
    check: Check,
    chevronDown: ChevronDown,
    chevronLeft: ChevronLeft,
    chevronRight: ChevronRight,
    chevronUp: ChevronUp,
    clock: Clock,
    compass: Compass,
    copy: Copy,
    album: Disc3,
    download: Download,
    external: ExternalLink,
    folder: Folder,
    folderOpen: FolderOpen,
    globe: Globe,
    drive: HardDrive,
    headphones: Headphones,
    heart: Heart,
    home: Home,
    info: Info,
    layers: Layers,
    library: Library,
    queue: ListMusic,
    addToQueue: ListPlus,
    maximize: Maximize2,
    lyrics: MicVocal,
    minimize: PictureInPicture2,
    moon: Moon,
    more: Ellipsis,
    music: Music,
    palette: Palette,
    pause: Pause,
    pin: Pin,
    play: Play,
    plus: Plus,
    podcast: Podcast,
    plugin: Puzzle,
    radio: Radio,
    refresh: RefreshCw,
    repeat: Repeat,
    repeatOne: Repeat1,
    search: Search,
    settings: Settings,
    share: Share2,
    store: ShoppingBag,
    shuffle: Shuffle,
    prev: SkipBack,
    next: SkipForward,
    eq: SlidersVertical,
    sparkles: Sparkles,
    speaker: Speaker,
    sun: Sun,
    terminal: Terminal,
    trash: Trash2,
    upload: Upload,
    artist: User,
    jam: Users,
    volumeLow: Volume1,
    volumeHigh: Volume2,
    volumeMute: VolumeX,
    wand: WandSparkles,
    close: X,
  };

  let IconComp = $derived(ICONS[name]);
</script>

{#if customSvg}
  <span
    class="inline-flex shrink-0 items-center justify-center [&>svg]:h-full [&>svg]:w-full {className}"
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
  >
    {@html customSvg}
  </span>
{:else if IconComp}
  <IconComp
    {size}
    strokeWidth={1.5}
    fill={filled ? 'currentColor' : 'none'}
    class="shrink-0 {className}"
    aria-hidden="true"
  />
{/if}
