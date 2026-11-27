import type { LyricLine, LyricsDocument } from '$src/state/types';

export function activeLineIndex(lines: LyricLine[], positionMs: number): number {
  for (let i = lines.length - 1; i >= 0; i--) {
    if (positionMs >= lines[i].startMs) return i;
  }
  return 0;
}

export function wordProgress(startMs: number, endMs: number, positionMs: number): number {
  if (positionMs <= startMs) return 0;
  if (positionMs >= endMs) return 1;
  const span = Math.max(1, endMs - startMs);
  return (positionMs - startMs) / span;
}

function xmlText(text: string): string {
  return text
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\ud800-\udfff\ufffe\uffff]/gu, '\ufffd')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
}

function timedWords(line: LyricLine, lineEnd: number): string | null {
  const words = line.words?.filter((word) => word.text.trim());
  if (!words?.length) return null;

  const text = line.text.trim();
  let offset = 0;
  let spans = '';
  for (const [index, word] of words.entries()) {
    const token = word.text.trim();
    const start = text.indexOf(token, offset);
    const gap = text.slice(offset, start);
    if (start < 0 || gap.trim() || !Number.isFinite(word.startMs)) return null;
    const begin = Math.max(line.startMs, word.startMs);
    const nextStart = words[index + 1]?.startMs ?? lineEnd;
    const end = word.endMs > begin && Number.isFinite(word.endMs)
      ? word.endMs
      : Math.max(begin + 1, Number.isFinite(nextStart) ? nextStart : lineEnd);
    spans += `${gap ? ' ' : ''}<span begin="${begin}ms" end="${end}ms">${xmlText(token)}</span>`;
    offset = start + token.length;
  }

  return text.slice(offset).trim() ? null : spans;
}

export function toTimedText(doc: LyricsDocument, durationMs: number, wordByWord: boolean): string | null {
  if (!doc.synced && !doc.wordSynced) return null;
  const lines = doc.lines
    .filter((line) => line.text.trim() && Number.isFinite(line.startMs) && line.startMs >= 0)
    .sort((a, b) => a.startMs - b.startMs);
  if (!lines.length) return null;

  const paragraphs = lines.map((line, index) => {
    const start = line.startMs;
    const nextStart = lines[index + 1]?.startMs ?? durationMs;
    const end = Number.isFinite(line.endMs) && line.endMs > start
      ? line.endMs
      : nextStart > start ? nextStart : start + 2000;
    const text = doc.wordSynced && wordByWord ? timedWords(line, end) : null;
    return `<p begin="${start}ms" end="${end}ms" dir="auto">${text ?? xmlText(line.text)}</p>`;
  });

  return `<tt xmlns="http://www.w3.org/ns/ttml"><body><div>${paragraphs.join('')}</div></body></tt>`;
}
