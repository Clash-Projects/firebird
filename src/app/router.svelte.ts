export interface RouteEntry {
  path: string;
  params: Record<string, string>;
}

class RouterState {
  history = $state<string[]>(['/home']);
  index = $state(0);

  path = $derived(this.history[this.index] ?? '/home');
  canGoBack = $derived(this.index > 0);
  canGoForward = $derived(this.index < this.history.length - 1);

  navigate(path: string) {
    const normalized = path.startsWith('/') ? path : `/${path}`;
    if (normalized === this.path) return;
    const next = this.history.slice(0, this.index + 1);
    next.push(normalized);
    this.history = next;
    this.index = next.length - 1;
  }

  back() {
    if (this.canGoBack) this.index -= 1;
  }

  forward() {
    if (this.canGoForward) this.index += 1;
  }

  matches(prefix: string): boolean {
    return this.path === prefix || this.path.startsWith(`${prefix}/`);
  }

  segment(idx: number): string {
    return this.path.split('/').filter(Boolean)[idx] ?? '';
  }
}

export const router = new RouterState();
