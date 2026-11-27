<script lang="ts">
  import Artwork from './Artwork.svelte';

  interface Props {
    posterUrl: string;
    title?: string;
    artist?: string;
    album?: string;
    videoUrls?: readonly string[];
    size?: number;
    radius?: number;
    class?: string;
  }

  let { posterUrl, title, artist, album, videoUrls = [], size, radius = 12, class: className = '' }: Props = $props();
  let failedUrls = $state<string[]>([]);
  let playingUrl = $state('');
  let activeUrl = $derived(videoUrls.find((url) => !failedUrls.includes(url)));
</script>

<div
  class="relative overflow-hidden {className}"
  style:width={size ? `${size}px` : undefined}
  style:height={size ? `${size}px` : undefined}
  style:border-radius="{radius}px"
>
  <Artwork
    src={posterUrl}
    label={title || album}
    kind={title ? 'track' : 'album'}
    {artist}
    {album}
    {size}
    {radius}
    highResolution
    class="h-full w-full"
  />
  {#if activeUrl}
    {#key activeUrl}
      <video
        src={activeUrl}
        autoplay
        loop
        muted
        playsinline
        onloadstart={() => { playingUrl = ''; }}
        onplaying={(event) => { playingUrl = event.currentTarget.getAttribute('src') ?? ''; }}
        onerror={(event) => {
          const failed = event.currentTarget.getAttribute('src');
          if (failed) failedUrls = [...failedUrls, failed];
        }}
        class="absolute inset-0 h-full w-full object-cover"
        class:opacity-0={playingUrl !== activeUrl}
      ></video>
    {/key}
  {/if}
</div>
