<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { playTrackNow } from '$src/app/trackActions';
  import { featuresStore } from '$src/state/features.svelte';
  import type { Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let { track, queue }: { track?: Track; queue: Track[] } = $props();

  function playSpotlight(radio = false) {
    if (!track) return;
    playTrackNow(track, radio ? [...queue].reverse() : queue);
    if (radio) featuresStore.showToast({ title: 'Started radio', subtitle: track.title, severity: 'info' });
  }
</script>

<section class="spotlight" class:featured={Boolean(track)} aria-labelledby="spotlight-title">
  <div class="spotlight-content">
    {#if track}
      <p class="spotlight-kicker"><WaveIcon name="radio" size={15} /> Featured mix</p>
      <h2 class="lw-display spotlight-title" id="spotlight-title" title={track.title}>{track.title}</h2>
      <p class="spotlight-description">{track.artist}</p>
      <div class="spotlight-actions">
        <button type="button" class="spotlight-primary" onclick={() => playSpotlight()}>
          <WaveIcon name="play" size={16} filled /> Play track
        </button>
        <button type="button" class="spotlight-secondary" onclick={() => playSpotlight(true)}>
          <WaveIcon name="radio" size={16} /> Start radio
        </button>
      </div>
    {:else}
      <p class="spotlight-kicker"><WaveIcon name="headphones" size={15} /> Your listening room</p>
      <h2 class="lw-display spotlight-title" id="spotlight-title">Find your next<br />favorite.</h2>
      <p class="spotlight-description">Explore YouTube Music or bring your own collection.</p>
      <div class="spotlight-actions">
        <button type="button" class="spotlight-primary" onclick={() => router.navigate('/search')}>
          <WaveIcon name="search" size={16} /> Find music
        </button>
        <button type="button" class="spotlight-secondary" onclick={() => router.navigate('/settings/library')}>
          <WaveIcon name="folder" size={16} /> Add a folder
        </button>
      </div>
    {/if}
  </div>

  {#if track?.artworkUrl}
    {#key track.id}
      <div class="spotlight-cover lw-enter" style:--lw-rise="7px">
        <Artwork src={track.artworkUrl} alt={`Cover art for ${track.title}`} radius={15} class="aspect-square w-full" />
      </div>
    {/key}
  {/if}
</section>

<style>
  .spotlight {
    --spotlight-accent: var(--lw-brand-green);
    --spotlight-base: #10170f;
    --spotlight-glow: #324824;
    --spotlight-border: #425637;

    position: relative;
    isolation: isolate;
    display: grid;
    min-height: 294px;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: clamp(20px, 4cqi, 52px);
    overflow: hidden;
    padding: clamp(25px, 3.2cqi, 40px) clamp(26px, 4.8cqi, 52px);
    border: 1px solid var(--spotlight-border);
    border-radius: 22px;
    background:
      radial-gradient(ellipse at 84% 72%, var(--spotlight-glow), transparent 64%),
      var(--spotlight-base);
    color: #f6f9f2;
  }

  :global(:root[data-theme="amoled"]) .spotlight {
    --spotlight-accent: #fff;
    --spotlight-base: #080808;
    --spotlight-glow: #232323;
    --spotlight-border: #333;
    color: #f5f5f5;
  }

  :global(:root[data-theme="volt"]) .spotlight {
    --spotlight-base: #050704;
    --spotlight-glow: #263719;
  }

  .spotlight::before {
    position: absolute;
    z-index: -2;
    inset: 50% 9% auto auto;
    width: clamp(260px, 43cqi, 530px);
    aspect-ratio: 1;
    background: url('../../../src-tauri/icons/icon.png') center / contain no-repeat;
    opacity: 0.34;
    transform: translateY(-47%) rotate(-13deg);
    content: '';
  }

  .spotlight.featured::before { right: 17%; }

  :global(:root[data-theme="amoled"]) .spotlight::before { filter: grayscale(1); }

  .spotlight::after {
    position: absolute;
    z-index: -1;
    inset: 0;
    background: linear-gradient(
      90deg,
      var(--spotlight-base),
      color-mix(in srgb, var(--spotlight-base) 74%, transparent) 34%,
      color-mix(in srgb, var(--spotlight-base) 23%, transparent) 65%,
      transparent 90%
    );
    content: '';
  }

  .spotlight-content { min-width: 0; }
  .spotlight-kicker {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 11px;
    color: var(--spotlight-accent);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.13em;
    text-transform: uppercase;
  }

  .spotlight-title {
    display: -webkit-box;
    overflow: hidden;
    font-size: clamp(36px, 6.4cqi, 70px);
    font-weight: 700;
    line-height: 0.98;
    overflow-wrap: anywhere;
    text-wrap: balance;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .featured .spotlight-title { font-size: clamp(31px, 5.8cqi, 62px); }
  .spotlight-description { max-width: 35ch; margin-top: 12px; color: color-mix(in srgb, currentColor 80%, var(--spotlight-glow)); font-size: clamp(13px, 1.5cqi, 15px); }
  .spotlight-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 9px; margin-top: 21px; }

  .spotlight-actions button {
    display: inline-flex;
    min-height: 40px;
    align-items: center;
    gap: 7px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: 9px;
    cursor: pointer;
    font-size: 12.5px;
    font-weight: 700;
    transition: background-color var(--lw-motion-fast), transform var(--lw-motion-fast) var(--ease-wave);
  }

  .spotlight-actions button:active { transform: scale(calc(1 - 0.03 * var(--lw-motion-factor))); }
  .spotlight-actions button:focus-visible { outline-color: var(--spotlight-accent); }
  .spotlight-actions .spotlight-primary { background: var(--spotlight-accent); color: var(--spotlight-base); }
  .spotlight-actions .spotlight-secondary {
    border-color: color-mix(in srgb, currentColor 30%, transparent);
    background: color-mix(in srgb, var(--spotlight-base) 70%, transparent);
    color: inherit;
  }
  .spotlight-cover {
    width: clamp(112px, 23cqi, 220px);
    overflow: hidden;
    border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
    border-radius: 16px;
    box-shadow: 0 16px 38px #0008;
  }

  @media (hover: hover) and (pointer: fine) {
    .spotlight-actions .spotlight-primary:hover { background: color-mix(in srgb, var(--spotlight-accent) 88%, var(--spotlight-glow)); }
    .spotlight-actions .spotlight-secondary:hover { background: var(--spotlight-glow); }
  }

  @container (max-width: 520px) {
    .spotlight { min-height: 240px; gap: 13px; padding: 21px; }
    .spotlight::before, .spotlight.featured::before { right: -10%; opacity: 0.22; }
    .spotlight-cover { width: clamp(78px, 24cqi, 125px); }
    .spotlight-actions { margin-top: 15px; }
  }

  @container (max-width: 370px) {
    .spotlight { grid-template-columns: 1fr; }
    .spotlight-cover { display: none; }
  }
</style>
