class ImmersiveState {
  chromeHidden = $state(false);
  private timer: ReturnType<typeof setTimeout> | null = null;

  ping(autoHideSec: number) {
    this.chromeHidden = false;
    if (this.timer) clearTimeout(this.timer);
    if (autoHideSec > 0) {
      this.timer = setTimeout(() => {
        this.chromeHidden = true;
      }, autoHideSec * 1000);
    }
  }

  reset() {
    if (this.timer) clearTimeout(this.timer);
    this.chromeHidden = false;
  }
}

export const immersiveState = new ImmersiveState();
