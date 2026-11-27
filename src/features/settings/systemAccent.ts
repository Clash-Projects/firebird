export function readSystemAccent(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const probe = document.createElement('div');
    probe.style.color = 'AccentColor';
    document.body.appendChild(probe);
    const computed = getComputedStyle(probe).color;
    probe.remove();
    const m = computed.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    if (!m) return null;
    const toHex = (n: string) => Number(n).toString(16).padStart(2, '0');
    return `#${toHex(m[1])}${toHex(m[2])}${toHex(m[3])}`;
  } catch {
    return null;
  }
}
