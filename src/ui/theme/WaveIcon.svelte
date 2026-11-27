<script module lang="ts">
  import {
    Activity,
    AlertCircle,
    ArrowLeft,
    ArrowRight,
    ArrowUpDown,
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
    Mic2,
    Minimize2,
    Moon,
    MoreHorizontal,
    Music,
    Palette,
    Pause,
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
    Sliders,
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
    Wand2,
    X,
  } from 'lucide-svelte';

  const ICONS = {
    activity: Activity,
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
    lyrics: Mic2,
    minimize: Minimize2,
    moon: Moon,
    more: MoreHorizontal,
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
    eq: Sliders,
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
    wand: Wand2,
    close: X,
  } as const;

  export type WaveIconName = keyof typeof ICONS;
</script>

<script lang="ts">
  interface Props {
    name: WaveIconName;
    size?: number;
    filled?: boolean;
    class?: string;
  }

  let { name, size = 18, filled = false, class: className = '' }: Props = $props();
  let Icon = $derived(ICONS[name] ?? Music);
  const FLUENT_NAV = new Set<WaveIconName>([
    'home',
    'stats',
    'search',
    'podcast',
    'queue',
    'library',
    'store',
    'jam',
    'settings',
  ]);
</script>

{#if FLUENT_NAV.has(name)}
  <svg
    width={size}
    height={size}
    viewBox="0 0 20 20"
    fill="none"
    class="shrink-0 {className}"
    aria-hidden="true"
  >
    {#if name === 'home'}
      <path
        d="M3.2 8.7L9.35 3.35C9.72 3.03 10.28 3.03 10.65 3.35L16.8 8.7C17.08 8.94 17.25 9.3 17.25 9.67V15.75C17.25 16.44 16.69 17 16 17H12.5V12.4C12.5 11.79 12.01 11.3 11.4 11.3H8.6C7.99 11.3 7.5 11.79 7.5 12.4V17H4C3.31 17 2.75 16.44 2.75 15.75V9.67C2.75 9.3 2.92 8.94 3.2 8.7Z"
        stroke="currentColor"
        stroke-width="1.35"
        stroke-linejoin="round"
      />
    {:else if name === 'stats'}
      <rect x="3" y="8" width="3.5" height="9" rx="0.7" stroke="currentColor" stroke-width="1.35" />
      <rect x="8.25" y="3" width="3.5" height="14" rx="0.7" stroke="currentColor" stroke-width="1.35" />
      <rect x="13.5" y="10.5" width="3.5" height="6.5" rx="0.7" stroke="currentColor" stroke-width="1.35" />
    {:else if name === 'search'}
      <circle cx="8.8" cy="8.8" r="5.6" stroke="currentColor" stroke-width="1.4" />
      <path d="M13 13L17 17" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
    {:else if name === 'podcast'}
      <path
        d="M4.2 11V9.5C4.2 6.3 6.8 3.7 10 3.7C13.2 3.7 15.8 6.3 15.8 9.5V11"
        stroke="currentColor"
        stroke-width="1.35"
        stroke-linecap="round"
      />
      <rect x="3" y="10" width="2.8" height="4.8" rx="1.2" stroke="currentColor" stroke-width="1.3" />
      <rect x="14.2" y="10" width="2.8" height="4.8" rx="1.2" stroke="currentColor" stroke-width="1.3" />
      <path
        d="M15.6 14.8C15.6 16.3 14.2 17.2 12 17.2H10.7"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
      />
      <circle cx="10" cy="17.2" r="0.9" fill="currentColor" />
    {:else if name === 'queue'}
      <path d="M2.5 5.2H12.5M2.5 9H9.5M2.5 12.8H7.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
      <path d="M13 13.8V7.2L17 6.2V12.2" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" />
      <circle cx="11.4" cy="14.2" r="1.7" fill="currentColor" />
      <circle cx="15.4" cy="12.6" r="1.7" fill="currentColor" />
    {:else if name === 'library'}
      <rect x="3" y="3.5" width="2.8" height="13" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect x="7.4" y="3.5" width="2.8" height="13" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect
        x="11.8"
        y="4.2"
        width="2.8"
        height="12.4"
        rx="0.7"
        transform="rotate(-11 13.2 10.4)"
        stroke="currentColor"
        stroke-width="1.3"
      />
    {:else if name === 'store'}
      <path
        d="M4.2 6.8H15.8L16.7 15.8C16.8 16.5 16.25 17.1 15.55 17.1H4.45C3.75 17.1 3.2 16.5 3.3 15.8L4.2 6.8Z"
        stroke="currentColor"
        stroke-width="1.35"
        stroke-linejoin="round"
      />
      <path
        d="M7.2 6.8V5.5C7.2 3.95 8.45 2.7 10 2.7C11.55 2.7 12.8 3.95 12.8 5.5V6.8"
        stroke="currentColor"
        stroke-width="1.35"
        stroke-linecap="round"
      />
    {:else if name === 'jam'}
      <circle cx="7.6" cy="6.7" r="2.7" stroke="currentColor" stroke-width="1.35" />
      <path
        d="M2.8 16C2.8 13.5 4.9 11.7 7.6 11.7C10.3 11.7 12.4 13.5 12.4 16"
        stroke="currentColor"
        stroke-width="1.35"
        stroke-linecap="round"
      />
      <circle cx="13.8" cy="7.3" r="2.1" stroke="currentColor" stroke-width="1.3" />
      <path
        d="M13.5 11.9C15.5 12 17.2 13.5 17.2 15.7"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
      />
    {:else if name === 'settings'}
      <circle cx="10" cy="10" r="2.6" stroke="currentColor" stroke-width="1.35" />
      <path
        d="M10 2.8L11.2 4.3C11.7 4.4 12.2 4.6 12.6 4.9L14.4 4.3L15.7 5.6L15.1 7.4C15.4 7.8 15.6 8.3 15.7 8.8L17.2 10L15.7 11.2C15.6 11.7 15.4 12.2 15.1 12.6L15.7 14.4L14.4 15.7L12.6 15.1C12.2 15.4 11.7 15.6 11.2 15.7L10 17.2L8.8 15.7C8.3 15.6 7.8 15.4 7.4 15.1L5.6 15.7L4.3 14.4L4.9 12.6C4.6 12.2 4.4 11.7 4.3 11.2L2.8 10L4.3 8.8C4.4 8.3 4.6 7.8 4.9 7.4L4.3 5.6L5.6 4.3L7.4 4.9C7.8 4.6 8.3 4.4 8.8 4.3L10 2.8Z"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linejoin="round"
      />
    {/if}
  </svg>
{:else}
  <Icon
    {size}
    strokeWidth={1.8}
    fill={filled ? 'currentColor' : 'none'}
    class="shrink-0 {className}"
  />
{/if}
