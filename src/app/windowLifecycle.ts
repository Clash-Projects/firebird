export function attachWindowLifecycle(onFocusChange?: (focused: boolean) => void): () => void {
  const onFocus = () => onFocusChange?.(true);
  const onBlur = () => onFocusChange?.(false);
  const onContextMenu = (event: MouseEvent) => {
    const target = event.target;
    if (
      target instanceof Element &&
      target.closest('input, textarea, [contenteditable="true"], [data-allow-context], .select-text')
    ) {
      return;
    }
    event.preventDefault();
  };

  window.addEventListener('focus', onFocus);
  window.addEventListener('blur', onBlur);
  window.addEventListener('contextmenu', onContextMenu);
  return () => {
    window.removeEventListener('focus', onFocus);
    window.removeEventListener('blur', onBlur);
    window.removeEventListener('contextmenu', onContextMenu);
  };
}
