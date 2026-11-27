<script lang="ts">
  import Artwork from './Artwork.svelte';

  interface Props {
    posterUrl: string;
    videoUrl?: string;
    size?: number;
    radius?: number;
    class?: string;
  }

  let { posterUrl, videoUrl, size, radius = 12, class: className = '' }: Props = $props();
</script>

<div
  class="relative overflow-hidden {className}"
  style:width={size ? `${size}px` : undefined}
  style:height={size ? `${size}px` : undefined}
  style:border-radius="{radius}px"
>
  {#if videoUrl}
    <video
      src={videoUrl}
      poster={posterUrl}
      autoplay
      loop
      muted
      playsinline
      class="h-full w-full object-cover"
    ></video>
  {:else}
    <Artwork src={posterUrl} {size} {radius} class="h-full w-full" />
  {/if}
</div>
