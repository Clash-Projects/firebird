<script module lang="ts">
  export type BrandIconName =
    | 'lastwave'
    | 'youtube'
    | 'spotify'
    | 'lastfm'
    | 'discord'
    | 'listenbrainz'
    | 'librefm'
    | 'apple'
    | 'tidal'
    | 'atmos'
    | 'hires';
</script>

<script lang="ts">
  interface Props {
    name: BrandIconName;
    size?: number;
    class?: string;
  }

  let { name, size = 18, class: className = '' }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  class="shrink-0 {className}"
  aria-hidden="true"
>
  {#if name === 'lastwave'}
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
  {:else if name === 'discord'}
    <rect x="2.5" y="4.5" width="19" height="15" rx="5" fill="#5865f2" />
    <circle cx="9.2" cy="12" r="1.6" fill="#fff" />
    <circle cx="14.8" cy="12" r="1.6" fill="#fff" />
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
