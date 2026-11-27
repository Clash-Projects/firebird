export function announcePolite(message: string) {
  let region = document.getElementById('lw-live-region');
  if (!region) {
    region = document.createElement('div');
    region.id = 'lw-live-region';
    region.setAttribute('aria-live', 'polite');
    region.setAttribute('aria-atomic', 'true');
    region.style.position = 'fixed';
    region.style.width = '1px';
    region.style.height = '1px';
    region.style.overflow = 'hidden';
    region.style.clipPath = 'inset(50%)';
    document.body.appendChild(region);
  }
  region.textContent = '';
  requestAnimationFrame(() => {
    if (region) region.textContent = message;
  });
}
