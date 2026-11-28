import type { LyricLine, LyricWord } from '$src/state/types';

export type VocalRole = 'leadLeft' | 'duetRight' | 'unisonCenter';

export interface KaraokeWordGroup {
  syllables: LyricWord[];
  hasTrailingSpace: boolean;
}

/** True when every cue shares the same timestamp (e.g. untimed 0ms transcripts). */
export function lyricsAreUntimed(lines: LyricLine[]): boolean {
  if (lines.length < 2) return false;
  return lines[0].startMs === lines[lines.length - 1].startMs;
}

/**
 * Index of the line currently being sung, or `-1` if playback is still before
 * the first cue or lyrics are untimed. Matches OG Flutter `activeLyricLineIndex` 1:1.
 */
export function activeLineIndex(
  lines: LyricLine[],
  positionMs: number,
  synced = true
): number {
  if (!synced || lines.length === 0) return -1;
  if (positionMs < lines[0].startMs) return -1;
  if (lyricsAreUntimed(lines)) return -1;

  if (lines.length > 1 && lines[0].startMs === lines[1].startMs) {
    let nextDistinctTime = -1;
    for (let i = 2; i < lines.length; i++) {
      if (lines[i].startMs !== lines[0].startMs) {
        nextDistinctTime = lines[i].startMs;
        break;
      }
    }
    if (nextDistinctTime >= 0 && positionMs < nextDistinctTime) {
      return 0;
    }
  }

  let low = 0;
  let high = lines.length - 1;
  let result = 0;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (lines[mid].startMs <= positionMs) {
      result = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return result;
}

export function wordProgress(startMs: number, endMs: number, positionMs: number): number {
  const dur = Math.max(1, endMs - startMs);
  if (positionMs <= startMs) return 0;
  if (positionMs >= startMs + dur) return 1;
  return (positionMs - startMs) / dur;
}

/** RTL detection for Arabic/Hebrew/Syriac lyrics rendering. */
export function isRtlText(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    const code = text.codePointAt(i) ?? 0;
    if (code > 0xffff) i++;
    if (
      (code >= 0x0590 && code <= 0x08ff) ||
      (code >= 0xfb00 && code <= 0xfdff) ||
      (code >= 0xfe70 && code <= 0xfeff)
    ) {
      return true;
    }
  }
  return false;
}

/**
 * True for scripts whose letters join or stack (Devanagari & other Indic scripts,
 * Thai, Lao, Khmer, Myanmar): negative letter-spacing makes adjacent glyphs overlap.
 */
export function isJoinedScript(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    const code = text.codePointAt(i) ?? 0;
    if (code > 0xffff) i++;
    if (
      (code >= 0x0900 && code <= 0x0dff) ||
      (code >= 0x0e00 && code <= 0x0eff) ||
      (code >= 0x1000 && code <= 0x109f) ||
      (code >= 0x1780 && code <= 0x17ff)
    ) {
      return true;
    }
  }
  return false;
}

/** True when `text` contains non-Latin script characters eligible for phonetic romanisation. */
export function needsRomanization(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    const u = text.codePointAt(i) ?? 0;
    if (u > 0xffff) i++;
    if (
      (u >= 0x0370 && u <= 0x03ff) || // Greek
      (u >= 0x0400 && u <= 0x052f) || // Cyrillic
      (u >= 0x0530 && u <= 0x08ff) || // Armenian, Hebrew, Arabic
      (u >= 0x0900 && u <= 0x0dff) || // Devanagari & Indic
      (u >= 0x0e00 && u <= 0x10ff) || // Thai, Lao, Tibetan, Myanmar, Georgian
      (u >= 0x1100 && u <= 0x11ff) || // Hangul Jamo
      (u >= 0x1780 && u <= 0x17ff) || // Khmer
      (u >= 0x3040 && u <= 0x30ff) || // Hiragana & Katakana
      (u >= 0x3400 && u <= 0x9fff) || // CJK Unified Ideographs
      (u >= 0xac00 && u <= 0xd7af) || // Hangul Syllables
      (u >= 0xf900 && u <= 0xfaff)
    ) {
      return true;
    }
  }
  return false;
}

/** Duration plausibility check matching OG Flutter `lyricsPlausibleDuration`. */
export function lyricsPlausibleDuration(
  lines: LyricLine[],
  durationSeconds: number
): boolean {
  if (!durationSeconds || durationSeconds <= 0) return true;
  if (lines.length < 3) return true;
  const expectedMs = durationSeconds * 1000;
  if (expectedMs < 60_000) return true;
  let endMs = 0;
  for (const line of lines) {
    let sylEnd = 0;
    if (line.words) {
      for (const w of line.words) {
        if (w.endMs > sylEnd) sylEnd = w.endMs;
      }
    }
    const e = Math.max(line.endMs, sylEnd);
    if (e > endMs) endMs = e;
  }
  if (endMs <= 0) return true;
  if (endMs > expectedMs + 45_000) return false;
  if (endMs < expectedMs * 0.45 && expectedMs - endMs > 90_000) return false;
  return true;
}

/** Synthesize proportional word syllables across a line's duration. */
export function interpolateLineWords(
  text: string,
  startTimeMs: number,
  durationMs: number
): LyricWord[] {
  const trimmed = text.trim();
  if (!trimmed) return [];
  const rawWords = trimmed.split(/\s+/).filter(Boolean);
  if (!rawWords.length) return [];
  const totalChars = rawWords.reduce((acc, w) => acc + w.length, 0);
  if (totalChars <= 0) return [];

  const safeDuration = durationMs > 0 ? durationMs : Math.max(1500, rawWords.length * 350);
  const out: LyricWord[] = [];
  let currentMs = startTimeMs;
  for (let i = 0; i < rawWords.length; i++) {
    const w = rawWords[i];
    const isLast = i === rawWords.length - 1;
    const dur = Math.max(120, Math.round((w.length / totalChars) * safeDuration));
    out.push({
      text: isLast ? w : `${w} `,
      startMs: currentMs,
      endMs: currentMs + dur,
    });
    currentMs += dur;
  }
  return out;
}

/** Group syllables into unbreakable word runs (matches OG `groupSyllablesIntoWords`). */
export function groupSyllablesIntoWords(
  syllables: LyricWord[],
  lineText: string
): KaraokeWordGroup[] {
  const groups: KaraokeWordGroup[] = [];
  let current: LyricWord[] = [];

  for (let i = 0; i < syllables.length; i++) {
    const syl = syllables[i];
    current.push(syl);

    let isWordEnd = false;
    if (syl.text.endsWith(' ')) {
      isWordEnd = true;
    } else if (i === syllables.length - 1) {
      isWordEnd = true;
    } else {
      const nextSyl = syllables[i + 1];
      if (nextSyl.text.startsWith(' ')) {
        isWordEnd = true;
      } else {
        const combined = `${syl.text}${nextSyl.text}`;
        if (!lineText.includes(combined)) {
          isWordEnd = true;
        }
      }
    }

    if (isWordEnd) {
      groups.push({
        syllables: current,
        hasTrailingSpace: i < syllables.length - 1,
      });
      current = [];
    }
  }

  if (current.length > 0) {
    groups.push({ syllables: current, hasTrailingSpace: false });
  }
  return groups;
}

/** Viewport alignment for follow-scroll (18% desktop golden-ratio upper-third anchor). */
export function lyricFollowAlignment(
  activeIdx: number,
  compact: boolean,
  targetAnchor = 0.18
): number {
  if (activeIdx <= 0) return 0;
  if (activeIdx === 1) return compact ? 0.12 : 0.14;
  return compact ? 0.24 : targetAnchor;
}

/** Spatial duet role detection matching `_computeLineRoles` in `karaoke_lyrics_view.dart`. */
export function computeVocalRoles(lines: LyricLine[], artist: string): VocalRole[] {
  const roles: VocalRole[] = new Array(lines.length).fill('leadLeft');
  if (!lines.length) return roles;

  const credits = artist
    .split(/\s*(?:,|&|\/|;|\+|\bfeat\.?|\bft\.?|\bfeaturing|\bwith|\bx)\s*/i)
    .map((c) => c.trim().toLowerCase())
    .filter(Boolean);
  const artist1 = credits[0] ?? '';
  const artist2 = credits[1] ?? '';

  let currentRole: VocalRole = 'leadLeft';
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const text = line.text.trim();
    const lower = text.toLowerCase();
    const agent = line.agent?.toLowerCase() ?? '';

    if (
      agent === 'v2' ||
      agent === 'agent2' ||
      agent === 'singer2' ||
      text.startsWith('Vocalist 2:') ||
      text.startsWith('Singer 2:') ||
      text.startsWith('Artist 2:') ||
      text.includes('[v2]') ||
      text.includes('(v2)')
    ) {
      currentRole = 'duetRight';
    } else if (
      agent === 'v1' ||
      agent === 'agent1' ||
      agent === 'singer1' ||
      text.startsWith('Vocalist 1:') ||
      text.startsWith('Singer 1:') ||
      text.startsWith('Artist 1:') ||
      text.includes('[v1]') ||
      text.includes('(v1)')
    ) {
      currentRole = 'leadLeft';
    } else if (
      agent.includes('both') ||
      text.startsWith('Both:') ||
      text.startsWith('Chorus:') ||
      text.includes('[all]') ||
      text.includes('[both]') ||
      text.includes('(both)')
    ) {
      currentRole = 'unisonCenter';
    } else if (
      artist2 &&
      (lower.startsWith(`[${artist2}`) ||
        lower.startsWith(`(${artist2}`) ||
        lower.startsWith(`${artist2}:`) ||
        (lower.includes('verse 2') && lower.includes(artist2)))
    ) {
      currentRole = 'duetRight';
    } else if (
      artist1 &&
      (lower.startsWith(`[${artist1}`) ||
        lower.startsWith(`(${artist1}`) ||
        lower.startsWith(`${artist1}:`) ||
        (lower.includes('verse 1') && lower.includes(artist1)))
    ) {
      currentRole = 'leadLeft';
    }

    roles[i] = currentRole;
  }

  return roles;
}
