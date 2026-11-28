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
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';

  interface Props {
    name: WaveIconName;
    size?: number;
    filled?: boolean;
    class?: string;
  }

  let { name, size = 18, filled = false, class: className = '' }: Props = $props();
  let customSvg = $derived(uiSlots.iconOverrides[name]);
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
{:else}
  <!-- Unified 20x20 Fluent / Segoe MDL2 1.3px geometric stroke set (matches WaveIcons in wave_icons.dart) -->
  <svg
    width={size}
    height={size}
    viewBox="0 0 20 20"
    fill="none"
    class="shrink-0 {className}"
    aria-hidden="true"
  >
    {#if name === 'home'}
      <path d="M3.5 9L10 3.3L16.5 9V16C16.5 16.55 16.05 17 15.5 17H12V11.8H8V17H4.5C3.95 17 3.5 16.55 3.5 16V9Z" stroke="currentColor" stroke-width="1.05" stroke-linejoin="round" />
    {:else if name === 'stats'}
      <rect x="3" y="8" width="3.5" height="9" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect x="8.25" y="3" width="3.5" height="14" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect x="13.5" y="10.5" width="3.5" height="6.5" rx="0.7" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'search'}
      <circle cx="8.8" cy="8.8" r="5.5" stroke="currentColor" stroke-width="1.1" />
      <path d="M13 13L17 17" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" />
    {:else if name === 'podcast'}
      <path d="M4.2 11V9.5C4.2 6.3 6.8 3.7 10 3.7C13.2 3.7 15.8 6.3 15.8 9.5V11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <rect x="3" y="10" width="2.8" height="4.8" rx="1.2" stroke="currentColor" stroke-width="1.3" />
      <rect x="14.2" y="10" width="2.8" height="4.8" rx="1.2" stroke="currentColor" stroke-width="1.3" />
      <path d="M15.6 14.8C15.6 16.3 14.2 17.2 12 17.2H10.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <circle cx="10" cy="17.2" r="0.9" fill="currentColor" />
    {:else if name === 'queue'}
      <path d="M2.5 5.2H12.5M2.5 9H9.5M2.5 12.8H7.5" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
      <path d="M13 13.8V7.2L17 6.2V12.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <circle cx="11.4" cy="14.2" r="1.6" fill="currentColor" />
      <circle cx="15.4" cy="12.6" r="1.6" fill="currentColor" />
    {:else if name === 'addToQueue'}
      <path d="M3 5.5H13M3 9.5H10M3 13.5H8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <path d="M14.5 9.5V16.5M11 13H18" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
    {:else if name === 'library'}
      <rect x="3" y="3.5" width="2.8" height="13" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect x="7.4" y="3.5" width="2.8" height="13" rx="0.7" stroke="currentColor" stroke-width="1.3" />
      <rect x="11.8" y="4.2" width="2.8" height="12.4" rx="0.7" transform="rotate(-11 13.2 10.4)" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'store'}
      <path d="M4.2 6.8H15.8L16.7 15.8C16.8 16.5 16.25 17.1 15.55 17.1H4.45C3.75 17.1 3.2 16.5 3.3 15.8L4.2 6.8Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
      <path d="M7.2 6.8V5.5C7.2 3.95 8.45 2.7 10 2.7C11.55 2.7 12.8 3.95 12.8 5.5V6.8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'jam'}
      <circle cx="7.6" cy="6.7" r="2.7" stroke="currentColor" stroke-width="1.3" />
      <path d="M2.8 16C2.8 13.5 4.9 11.7 7.6 11.7C10.3 11.7 12.4 13.5 12.4 16" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <circle cx="13.8" cy="7.3" r="2.1" stroke="currentColor" stroke-width="1.25" />
      <path d="M13.5 11.9C15.5 12 17.2 13.5 17.2 15.7" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" />
    {:else if name === 'settings'}
      <circle cx="10" cy="10" r="2.6" stroke="currentColor" stroke-width="1.3" />
      <path d="M10 2.8L11.2 4.3C11.7 4.4 12.2 4.6 12.6 4.9L14.4 4.3L15.7 5.6L15.1 7.4C15.4 7.8 15.6 8.3 15.7 8.8L17.2 10L15.7 11.2C15.6 11.7 15.4 12.2 15.1 12.6L15.7 14.4L14.4 15.7L12.6 15.1C12.2 15.4 11.7 15.6 11.2 15.7L10 17.2L8.8 15.7C8.3 15.6 7.8 15.4 7.4 15.1L5.6 15.7L4.3 14.4L4.9 12.6C4.6 12.2 4.4 11.7 4.3 11.2L2.8 10L4.3 8.8C4.4 8.3 4.6 7.8 4.9 7.4L4.3 5.6L5.6 4.3L7.4 4.9C7.8 4.6 8.3 4.4 8.8 4.3L10 2.8Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" />
    {:else if name === 'play'}
      <path d="M6.2 4.4C5.7 4.1 5 4.46 5 5.06V14.94C5 15.54 5.7 15.9 6.2 15.6L15.1 10.66C15.62 10.37 15.62 9.63 15.1 9.34L6.2 4.4Z" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'pause'}
      <rect x="5.5" y="4.2" width="2.8" height="11.6" rx="0.8" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" />
      <rect x="11.7" y="4.2" width="2.8" height="11.6" rx="0.8" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'prev'}
      <path d="M4.2 4.8V15.2" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" />
      <path d="M15.2 5.2L7.4 10L15.2 14.8V5.2Z" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'next'}
      <path d="M15.8 4.8V15.2" stroke="currentColor" stroke-width="1.45" stroke-linecap="round" />
      <path d="M4.8 5.2L12.6 10L4.8 14.8V5.2Z" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'shuffle'}
      <path d="M3 6H6.2C7.4 6 8.3 6.6 9.1 7.6L11.4 10.7C12.2 11.7 13.1 12.3 14.3 12.3H17M17 12.3L14.7 10M17 12.3L14.7 14.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <path d="M17 6H14.3C13.1 6 12.2 6.6 11.4 7.6L10.8 8.4M9.1 11.5C8.3 12.5 7.4 13.1 6.2 13.1H3M17 6L14.7 3.7M17 6L14.7 8.3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'repeat'}
      <path d="M14.8 4.3L17 6.5L14.8 8.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <path d="M3.2 10.5V8.5C3.2 7.4 4.1 6.5 5.2 6.5H16.8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <path d="M5.2 15.7L3 13.5L5.2 11.3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <path d="M16.8 9.5V11.5C16.8 12.6 15.9 13.5 14.8 13.5H3.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'repeatOne'}
      <path d="M14.8 4.3L17 6.5L14.8 8.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <path d="M3.2 10.5V8.5C3.2 7.4 4.1 6.5 5.2 6.5H16.8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <path d="M5.2 15.7L3 13.5L5.2 11.3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <path d="M16.8 9.5V11.5C16.8 12.6 15.9 13.5 14.8 13.5H3.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <path d="M10 8V12.2M9 9.1L10 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'heart'}
      <path d="M10 16.5C10 16.5 3.3 12.3 3.3 7.4C3.3 5.3 4.9 3.8 6.9 3.8C8.2 3.8 9.3 4.5 10 5.6C10.7 4.5 11.8 3.8 13.1 3.8C15.1 3.8 16.7 5.3 16.7 7.4C16.7 12.3 10 16.5 10 16.5Z" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'more'}
      <circle cx="4.5" cy="10" r="1.15" fill="currentColor" />
      <circle cx="10" cy="10" r="1.15" fill="currentColor" />
      <circle cx="15.5" cy="10" r="1.15" fill="currentColor" />
    {:else if name === 'lyrics'}
      <path d="M4.2 5.2H12.5M4.2 9.5H10.8M4.2 13.8H8.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <path d="M14.8 13.8V6.2L17.4 5.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <circle cx="13.2" cy="14" r="1.6" fill="currentColor" />
    {:else if name === 'eq'}
      <path d="M4.5 4V16M10 4V16M15.5 4V16" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" />
      <circle cx="4.5" cy="7.5" r="1.7" fill="var(--lw-bg, #0e0e0e)" stroke="currentColor" stroke-width="1.3" />
      <circle cx="10" cy="12.5" r="1.7" fill="var(--lw-bg, #0e0e0e)" stroke="currentColor" stroke-width="1.3" />
      <circle cx="15.5" cy="8.5" r="1.7" fill="var(--lw-bg, #0e0e0e)" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'speaker'}
      <rect x="5" y="2.8" width="10" height="14.4" rx="1.8" stroke="currentColor" stroke-width="1.3" />
      <circle cx="10" cy="6.4" r="1.2" stroke="currentColor" stroke-width="1.2" />
      <circle cx="10" cy="12.3" r="2.7" stroke="currentColor" stroke-width="1.25" />
    {:else if name === 'headphones'}
      <path d="M3.8 12V10C3.8 6.6 6.6 3.8 10 3.8C13.4 3.8 16.2 6.6 16.2 10V12" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
      <rect x="3.2" y="10.8" width="2.8" height="5.2" rx="1.2" stroke="currentColor" stroke-width="1.3" />
      <rect x="14" y="10.8" width="2.8" height="5.2" rx="1.2" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'volumeHigh'}
      <path d="M3.2 8.2H5.9L9.8 4.8V15.2L5.9 11.8H3.2V8.2Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
      <path d="M12.5 7.4C13.4 8.2 13.8 9 13.8 10C13.8 11 13.4 11.8 12.5 12.6M14.9 5.3C16.3 6.6 17 8.2 17 10C17 11.8 16.3 13.4 14.9 14.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'volumeLow'}
      <path d="M3.5 8.2H6.2L10.1 4.8V15.2L6.2 11.8H3.5V8.2Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
      <path d="M13 7.3C14 8.1 14.4 9 14.4 10C14.4 11 14 11.9 13 12.7" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'volumeMute'}
      <path d="M3.2 8.2H5.8L9.6 4.8V15.2L5.8 11.8H3.2V8.2Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
      <path d="M12.6 7.6L17 12.4M17 7.6L12.6 12.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'maximize'}
      <path d="M12 3.5H16.5V8M8 16.5H3.5V12M16.3 3.7L11.5 8.5M3.7 16.3L8.5 11.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'minimize'}
      <rect x="3" y="3.8" width="14" height="12.4" rx="1.8" stroke="currentColor" stroke-width="1.3" />
      <rect x="9.8" y="9.8" width="5.5" height="4.5" rx="0.9" stroke="currentColor" stroke-width="1.25" />
    {:else if name === 'close'}
      <path d="M4.8 4.8L15.2 15.2M15.2 4.8L4.8 15.2" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
    {:else if name === 'chevronLeft'}
      <path d="M12.2 4.6L6.8 10L12.2 15.4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'chevronRight'}
      <path d="M7.8 4.6L13.2 10L7.8 15.4" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'chevronUp'}
      <path d="M4.6 12.2L10 6.8L15.4 12.2" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'chevronDown'}
      <path d="M4.6 7.8L10 13.2L15.4 7.8" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'arrowLeft'}
      <path d="M16.5 10H3.8M8.8 5L3.8 10L8.8 15" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'arrowRight'}
      <path d="M3.5 10H16.2M11.2 5L16.2 10L11.2 15" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'album'}
      <!-- Vinyl-in-square WindowsIcons.music_album -->
      <rect x="3" y="3" width="14" height="14" rx="1.6" stroke="currentColor" stroke-width="1.3" />
      <circle cx="10" cy="10" r="4" stroke="currentColor" stroke-width="1.2" />
      <circle cx="10" cy="10" r="1.2" fill="currentColor" />
    {:else if name === 'artist'}
      <circle cx="10" cy="6.8" r="3.1" stroke="currentColor" stroke-width="1.3" />
      <path d="M4.2 16.5C4.2 13.6 6.8 11.7 10 11.7C13.2 11.7 15.8 13.6 15.8 16.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'music'}
      <path d="M8.5 14.5V5L16 3.5V12.8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
      <circle cx="6.7" cy="14.6" r="2" stroke="currentColor" stroke-width="1.3" />
      <circle cx="14.2" cy="13" r="2" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'plus'}
      <path d="M10 4V16M4 10H16" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
    {:else if name === 'check'}
      <path d="M4.2 10.4L8.1 14.3L16 6.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'download'}
      <path d="M10 3.5V12.8M6.2 9.4L10 13.2L13.8 9.4M4 16.2H16" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'upload'}
      <path d="M10 13.2V3.8M6.2 7.2L10 3.5L13.8 7.2M4 16.2H16" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'trash'}
      <path d="M4 5.8H16M7.8 3.3H12.2M6 5.8L6.6 16.2H13.4L14 5.8M8.5 8.8V13.4M11.5 8.8V13.4" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'clock'}
      <circle cx="10" cy="10" r="6.7" stroke="currentColor" stroke-width="1.3" />
      <path d="M10 6.2V10.3L12.8 12" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'refresh'}
      <path d="M16 9.8C16 13.2 13.3 16 10 16C6.7 16 4 13.2 4 9.8C4 6.5 6.7 3.8 10 3.8C12 3.8 13.8 4.8 14.9 6.3M15.6 3.5V6.6H12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'activity'}
      <path d="M2.8 12.8V7.2M6.4 15.6V4.4M10 14V6M13.6 16V4M17.2 12.2V7.8" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
    {:else if name === 'alert'}
      <circle cx="10" cy="10" r="6.7" stroke="currentColor" stroke-width="1.3" />
      <path d="M10 6.5V10.6" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
      <circle cx="10" cy="13.5" r="0.8" fill="currentColor" />
    {:else if name === 'info'}
      <circle cx="10" cy="10" r="6.7" stroke="currentColor" stroke-width="1.3" />
      <path d="M10 9.2V13.8" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" />
      <circle cx="10" cy="6.6" r="0.8" fill="currentColor" />
    {:else if name === 'sort'}
      <path d="M6.8 4.5V15.5M4.2 7L6.8 4.4L9.4 7M13.2 15.5V4.5M10.6 13L13.2 15.6L15.8 13" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'bookmark'}
      <path d="M5.5 3.8H14.5V16.6L10 13.4L5.5 16.6V3.8Z" fill={filled ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'pin'}
      <path d="M7 4.2H13L12.3 9.2L14.5 11.4V12.6H5.5V11.4L7.7 9.2L7 4.2ZM10 12.6V17" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'globe'}
      <circle cx="10" cy="10" r="6.7" stroke="currentColor" stroke-width="1.3" />
      <ellipse cx="10" cy="10" rx="3" ry="6.7" stroke="currentColor" stroke-width="1.2" />
      <path d="M3.5 10H16.5" stroke="currentColor" stroke-width="1.2" />
    {:else if name === 'compass'}
      <circle cx="10" cy="10" r="6.7" stroke="currentColor" stroke-width="1.3" />
      <path d="M12.6 7.4L11 11L7.4 12.6L9 9L12.6 7.4Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" />
    {:else if name === 'copy'}
      <rect x="7" y="6.8" width="9.2" height="9.8" rx="1.4" stroke="currentColor" stroke-width="1.3" />
      <path d="M13 6.8V4.6C13 3.9 12.5 3.4 11.8 3.4H5C4.3 3.4 3.8 3.9 3.8 4.6V12.2C3.8 12.9 4.3 13.4 5 13.4H7" stroke="currentColor" stroke-width="1.3" />
    {:else if name === 'external'}
      <path d="M11 4H16V9M15.8 4.2L9.2 10.8M15 11.5V15C15 15.7 14.5 16.2 13.8 16.2H5C4.3 16.2 3.8 15.7 3.8 15V6.2C3.8 5.5 4.3 5 5 5H8.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'folder' || name === 'folderOpen'}
      <path d="M3 5.2C3 4.5 3.5 4 4.2 4H8L9.6 5.8H15.8C16.5 5.8 17 6.3 17 7V15C17 15.7 16.5 16.2 15.8 16.2H4.2C3.5 16.2 3 15.7 3 15V5.2Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'drive'}
      <rect x="3" y="5.5" width="14" height="9" rx="1.6" stroke="currentColor" stroke-width="1.3" />
      <path d="M3 11.5H17" stroke="currentColor" stroke-width="1.25" />
      <circle cx="14" cy="13" r="0.8" fill="currentColor" />
    {:else if name === 'layers'}
      <path d="M10 3.8L16.5 7.2L10 10.6L3.5 7.2L10 3.8ZM3.5 10.3L10 13.7L16.5 10.3M3.5 13.2L10 16.6L16.5 13.2" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'moon'}
      <path d="M15.5 12.2C14.6 12.5 13.6 12.7 12.6 12.7C8.9 12.7 6 9.8 6 6.1C6 5.1 6.2 4.1 6.6 3.2C4.5 4.2 3.2 6.4 3.2 8.9C3.2 12.8 6.4 16 10.3 16C12.7 16 14.6 14.4 15.5 12.2Z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
    {:else if name === 'sun'}
      <circle cx="10" cy="10" r="3.4" stroke="currentColor" stroke-width="1.3" />
      <path d="M10 2.8V4.3M10 15.7V17.2M2.8 10H4.3M15.7 10H17.2M4.9 4.9L6 6M14 14L15.1 15.1M15.1 4.9L14 6M6 14L4.9 15.1" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
    {:else if name === 'palette'}
      <path d="M10 3.3C6.1 3.3 3.2 6.2 3.2 10C3.2 13.8 6.1 16.7 9.5 16.7C10.5 16.7 11.2 16 11.2 15.1C11.2 14.6 11 14.2 10.7 13.9C10.4 13.6 10.2 13.2 10.2 12.7C10.2 11.8 10.9 11.1 11.8 11.1H13.6C15.5 11.1 16.8 9.6 16.8 7.8C16.8 5.2 13.7 3.3 10 3.3Z" stroke="currentColor" stroke-width="1.3" />
      <circle cx="6.8" cy="9.2" r="0.9" fill="currentColor" />
      <circle cx="8.8" cy="6.6" r="0.9" fill="currentColor" />
      <circle cx="12.2" cy="6.9" r="0.9" fill="currentColor" />
    {:else if name === 'plugin'}
      <path d="M11.8 3.5C12.7 3.5 13.4 4.2 13.4 5.1V6H15.5C16.1 6 16.6 6.5 16.6 7.1V9.3H15.7C14.8 9.3 14.1 10 14.1 10.9C14.1 11.8 14.8 12.5 15.7 12.5H16.6V15.5C16.6 16.1 16.1 16.6 15.5 16.6H12.5V15.7C12.5 14.8 11.8 14.1 10.9 14.1C10 14.1 9.3 14.8 9.3 15.7V16.6H5.1C4.5 16.6 4 16.1 4 15.5V12.5H4.9C5.8 12.5 6.5 11.8 6.5 10.9C6.5 10 5.8 9.3 4.9 9.3H4V7.1C4 6.5 4.5 6 5.1 6H10.2V5.1C10.2 4.2 10.9 3.5 11.8 3.5Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" />
    {:else if name === 'radio'}
      <circle cx="10" cy="10" r="1.8" fill="currentColor" />
      <path d="M6.8 6.8C5.9 7.7 5.5 8.8 5.5 10C5.5 11.2 5.9 12.3 6.8 13.2M13.2 6.8C14.1 7.7 14.5 8.8 14.5 10C14.5 11.2 14.1 12.3 13.2 13.2M4.5 4.5C3.1 5.9 2.5 7.9 2.5 10C2.5 12.1 3.1 14.1 4.5 15.5M15.5 4.5C16.9 5.9 17.5 7.9 17.5 10C17.5 12.1 16.9 14.1 15.5 15.5" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" />
    {:else if name === 'share'}
      <circle cx="14.5" cy="5.2" r="2" stroke="currentColor" stroke-width="1.3" />
      <circle cx="5.5" cy="10" r="2" stroke="currentColor" stroke-width="1.3" />
      <circle cx="14.5" cy="14.8" r="2" stroke="currentColor" stroke-width="1.3" />
      <path d="M7.3 9.1L12.7 6.1M7.3 10.9L12.7 13.9" stroke="currentColor" stroke-width="1.25" />
    {:else if name === 'terminal'}
      <path d="M4 6.5L8.2 10L4 13.5M10 13.5H16" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" />
    {:else if name === 'sparkles' || name === 'wand'}
      <path d="M10 3L11.3 7.5L15.8 8.8L11.3 10.1L10 14.6L8.7 10.1L4.2 8.8L8.7 7.5L10 3Z" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" />
      <path d="M15.2 13.5V16.5M13.7 15H16.7" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" />
    {/if}
  </svg>
{/if}
