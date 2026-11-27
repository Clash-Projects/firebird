import { waveBreakpointFor, type WaveBreakpoint } from '$src/ui/theme/tokens';

class BreakpointState {
  width = $state(typeof window !== 'undefined' ? window.innerWidth : 1280);
  height = $state(typeof window !== 'undefined' ? window.innerHeight : 800);
  current = $derived<WaveBreakpoint>(waveBreakpointFor(this.width));
  isCompact = $derived(this.current === 'compact');
  isExpanded = $derived(this.current === 'expanded' || this.current === 'ultrawide');

  constructor() {
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', () => {
        this.width = window.innerWidth;
        this.height = window.innerHeight;
      });
    }
  }
}

export const viewport = new BreakpointState();
