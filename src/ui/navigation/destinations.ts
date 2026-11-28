import type { WaveIconName } from '$src/ui/theme/WaveIcon.svelte';

export interface NavDestination {
  id: string;
  route: string;
  labelKey: string;
  fallbackLabel: string;
  icon: WaveIconName;
  group: 'listen' | 'collection' | 'offline' | 'system';
}

export const NAV_DESTINATIONS: NavDestination[] = [
  { id: 'home', route: '/home', labelKey: 'navHome', fallbackLabel: 'Home', icon: 'home', group: 'listen' },
  { id: 'stats', route: '/stats', labelKey: 'navStats', fallbackLabel: 'Stats', icon: 'stats', group: 'listen' },
  { id: 'search', route: '/search', labelKey: 'navSearch', fallbackLabel: 'Search', icon: 'search', group: 'listen' },

  { id: 'podcasts', route: '/podcasts', labelKey: 'navPodcasts', fallbackLabel: 'Podcasts', icon: 'podcast', group: 'collection' },
  { id: 'playlists', route: '/playlists', labelKey: 'navPlaylists', fallbackLabel: 'Playlists', icon: 'playlists', group: 'collection' },

  { id: 'library', route: '/library', labelKey: 'navLibrary', fallbackLabel: 'Library', icon: 'library', group: 'offline' },

  { id: 'store', route: '/store', labelKey: 'navStore', fallbackLabel: 'Store', icon: 'store', group: 'system' },
  { id: 'jam', route: '/jam', labelKey: 'navJam', fallbackLabel: 'Jam', icon: 'jam', group: 'system' },
  { id: 'settings', route: '/settings', labelKey: 'navSettings', fallbackLabel: 'Settings', icon: 'settings', group: 'system' },
];
