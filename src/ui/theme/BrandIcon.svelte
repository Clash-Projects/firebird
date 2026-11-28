<script module lang="ts">
  export type BrandIconName =
    | 'lastwave'
    | 'telegram'
    | 'discord'
    | 'youtube'
    | 'spotify'
    | 'lastfm'
    | 'listenbrainz'
    | 'librefm'
    | 'apple'
    | 'tidal'
    | 'atmos'
    | 'hires';

  // Exact official Simple Icons 24x24 paths from OG Flutter `lib/ui/theme/brand_icons.dart`
  const TELEGRAM_D =
    'M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z';

  const DISCORD_D =
    'M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z';
</script>

<script lang="ts">
  interface Props {
    name: BrandIconName;
    size?: number;
    class?: string;
  }

  let { name, size = 15, class: className = '' }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  class="shrink-0 {className}"
  aria-hidden="true"
>
  {#if name === 'telegram'}
    <path d={TELEGRAM_D} fill="currentColor" />
  {:else if name === 'discord'}
    <path d={DISCORD_D} fill="currentColor" />
  {:else if name === 'lastwave'}
    <path
      d="M3 13.5C5.2 13.5 5.8 7 8 7C10.2 7 10.6 17 12.8 17C15 17 15.6 9.5 17.8 9.5C19.5 9.5 20.1 13.5 21 13.5"
      stroke="currentColor"
      stroke-width="2.2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  {:else if name === 'youtube'}
    <circle cx="12" cy="12" r="9.5" stroke="#ff3b5c" stroke-width="1.8" />
    <polygon points="10,8.5 16,12 10,15.5" fill="#ff3b5c" />
  {:else if name === 'spotify'}
    <circle cx="12" cy="12" r="9.5" fill="#1db954" />
    <path d="M7.5 10.2C10.8 9.2 14.3 9.5 17 11" stroke="#0b0b0b" stroke-width="1.7" stroke-linecap="round" />
    <path d="M8.3 13C10.9 12.3 13.7 12.5 15.9 13.7" stroke="#0b0b0b" stroke-width="1.5" stroke-linecap="round" />
    <path d="M9.2 15.6C11.2 15.1 13.3 15.3 14.9 16.2" stroke="#0b0b0b" stroke-width="1.4" stroke-linecap="round" />
  {:else if name === 'lastfm'}
    <rect x="2.5" y="2.5" width="19" height="19" rx="5" fill="#d51007" />
    <path d="M7 15.5V8.5M7 15.5H11M13 15.5V9.5C13 8.8 13.6 8.5 14.5 8.5H17M13 12H16.2" stroke="#fff" stroke-width="1.7" stroke-linecap="round" />
  {:else if name === 'atmos'}
    <rect x="2" y="6" width="8.5" height="12" rx="1.5" stroke="currentColor" stroke-width="1.6" />
    <path d="M5 9.2H6.4C7.8 9.2 8.6 10.4 8.6 12C8.6 13.6 7.8 14.8 6.4 14.8H5V9.2Z" fill="currentColor" />
    <rect x="13.5" y="6" width="8.5" height="12" rx="1.5" stroke="currentColor" stroke-width="1.6" />
    <path d="M19 9.2H17.6C16.2 9.2 15.4 10.4 15.4 12C15.4 13.6 16.2 14.8 17.6 14.8H19V9.2Z" fill="currentColor" />
  {:else if name === 'hires'}
    <rect x="2" y="4" width="20" height="16" rx="3" stroke="#e0a030" stroke-width="1.6" />
    <text x="12" y="14.8" text-anchor="middle" fill="#e0a030" font-size="7.5" font-weight="700" font-family="system-ui">HR</text>
  {:else}
    <circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.8" />
    <circle cx="12" cy="12" r="3" fill="currentColor" />
  {/if}
</svg>
