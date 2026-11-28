<script lang="ts">
  import { untrack } from 'svelte';
  import { activeLineIndex } from '$src/features/lyrics/lyricAdapter';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import type { LyricsDocument } from '$src/state/types';
  import { waveSlide } from '$src/ui/theme/motion';
  import { WaveMotion } from '$src/ui/theme/tokens';

  interface PlaybackSample {
    trackId: string;
    doc: LyricsDocument | null;
    positionMs: number;
    offsetMs: number;
    seekRevision: number;
    advancing: boolean;
  }

  const clockCorrectionMs = 500;
  let { trackId, seekPreview }: { trackId: string; seekPreview: number | null } = $props();
  let previous: PlaybackSample | null = null;
  let lyric = $state({ text: '♪', animate: false });

  $effect(() => {
    const doc = featuresStore.getLyrics(trackId);
    const sample: PlaybackSample = {
      trackId,
      doc,
      positionMs: (seekPreview ?? playerStore.positionSec) * 1000,
      offsetMs: featuresStore.getLyricsOffset(trackId),
      seekRevision: playerStore.seekRevision,
      advancing: seekPreview === null && playerStore.isPlaying && !playerStore.isBuffering && !playerStore.seeking,
    };
    const last = previous;
    const continuous = last && sample.advancing && last.advancing &&
      last.trackId === sample.trackId && last.doc === doc &&
      last.offsetMs === sample.offsetMs && last.seekRevision === sample.seekRevision &&
      Math.abs(sample.positionMs - last.positionMs) <= clockCorrectionMs;

    // Playback snapshots can briefly lag the interpolated clock near a lyric boundary.
    if (continuous) sample.positionMs = Math.max(last.positionMs, sample.positionMs);
    previous = sample;

    const index = doc?.synced && !doc.isInstrumental
      ? activeLineIndex(doc.lines, sample.positionMs - sample.offsetMs)
      : -1;
    const text = doc?.lines[index]?.text.trim() || '♪';
    if (text !== untrack(() => lyric.text)) lyric = { text, animate: Boolean(continuous) };
  });
</script>

<div data-lw="mini-player-lyrics" class="min-w-0 overflow-hidden rounded-[6px] bg-white/10 px-2 py-1 text-[11.5px] font-medium leading-4 text-white/95">
  {#key lyric.text}
    <span
      dir="auto"
      class="block truncate"
      in:waveSlide={{ y: 2, opacity: 0.74, duration: lyric.animate ? WaveMotion.fastMs : 0 }}
    >{lyric.text}</span>
  {/key}
</div>
