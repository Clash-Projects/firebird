class RouteOriginState {
  lastRect = $state<{ x: number; y: number; width: number; height: number } | null>(null);

  capture(el: HTMLElement | null) {
    if (!el) return;
    const r = el.getBoundingClientRect();
    this.lastRect = { x: r.left, y: r.top, width: r.width, height: r.height };
  }

  clear() {
    this.lastRect = null;
  }
}

export const routeOrigin = new RouteOriginState();
