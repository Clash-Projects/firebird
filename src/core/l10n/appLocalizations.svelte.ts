import { settingsStore } from '$src/state/settings.svelte';
import { TRANSLATIONS } from './translations';

export function t(key: string, fallback?: string): string {
  const lang = settingsStore.language || 'en';
  return TRANSLATIONS[lang]?.[key] ?? TRANSLATIONS.en[key] ?? fallback ?? key;
}
