<script lang="ts">
  import type { Snippet } from 'svelte';
  import Artwork from './Artwork.svelte';

  interface Props {
    overline?: string;
    title: string;
    subtitle?: string;
    meta?: string;
    artworkUrl?: string;
    circleArt?: boolean;
    actions?: Snippet;
  }

  let {
    overline,
    title,
    subtitle,
    meta,
    artworkUrl,
    circleArt = false,
    actions,
  }: Props = $props();
</script>

<header data-lw="hero-header" class="flex flex-col items-start gap-3 pb-5 sm:flex-row sm:items-end sm:gap-5">
  {#if artworkUrl !== undefined}
    <Artwork
      src={artworkUrl}
      label={title}
      kind={circleArt ? 'artist' : 'album'}
      artist={circleArt ? title : subtitle ?? ''}
      alt={title}
      size={168}
      radius={12}
      circle={circleArt}
      shadow="artwork"
    />
  {/if}
  <div class="min-w-0 flex-1">
    {#if overline}
      <div class="text-[10.5px] font-bold tracking-[1px] text-[var(--lw-accent)] uppercase">
        {overline}
      </div>
    {/if}
    <h1 class="mt-1 line-clamp-2 text-[22px] font-bold tracking-[-0.4px] leading-[1.15] text-[var(--lw-text-primary)]">
      {title}
    </h1>
    {#if subtitle}
      <p class="mt-1 max-w-[680px] text-[13px] text-[var(--lw-text-secondary)]">{subtitle}</p>
    {/if}
    {#if meta}
      <div class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">{meta}</div>
    {/if}
    {#if actions}
      <div class="mt-3 flex flex-wrap items-center gap-2">
        {@render actions()}
      </div>
    {/if}
  </div>
</header>
