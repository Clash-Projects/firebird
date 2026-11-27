export function attachWindowLifecycle(onFocusChange?: (focused: boolean) => void): () => void {
  const onFocus = () => onFocusChange?.(true);
  const onBlur = () => onFocusChange?.(false);
  window.addEventListener('focus', onFocus);
  window.addEventListener('blur', onBlur);
  return () => {
    window.removeEventListener('focus', onFocus);
    window.removeEventListener('blur', onBlur);
  };
}
