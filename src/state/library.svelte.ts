import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
import type {
  Album,
  Artist,
  DownloadItem,
  GenreItem,
  HomeEntityItem,
  HomeShelfItem,
  Playlist,
  Track,
} from './types';

interface BackendYtTrack {
  videoId: string;
  title: string;
  artist: string;
  album?: string;
  artworkUrl?: string;
  durationSeconds?: number;
  albumBrowseId?: string;
}

interface BackendYtEntity {
  kind: 'artist' | 'album' | 'playlist' | string;
  name: string;
  artist?: string;
  subtitle?: string;
  browseId?: string;
  playlistId?: string;
  artworkUrl?: string;
}

interface BackendYtHomeShelf {
  title: string;
  subtitle?: string;
  tracks?: BackendYtTrack[];
  trackCards?: BackendYtTrack[];
  entities?: BackendYtEntity[];
}

interface BackendStoredTrack {
  name: string;
  artist: string;
  artworkUrl?: string;
  videoId?: string;
  album?: string;
  durationSeconds?: number;
  playbackUrl?: string;
}

interface BackendSavedPlaylist {
  id: number;
  title: string;
  subtitle?: string;
  mode?: string;
  tracks?: BackendStoredTrack[];
  isPinned?: boolean;
  customCoverUri?: string;
}

interface BackendDownloadEntry {
  videoId?: string;
  trackKey?: string;
  status?: string;
  progress?: number;
  sizeBytes?: number;
  localPath?: string;
}

export function mapYtTrack(t: BackendYtTrack): Track {
  const id = t.videoId || `${t.title}-${t.artist}`;
  return {
    id,
    videoId: t.videoId,
    title: t.title || 'Unknown Title',
    artist: t.artist || 'Unknown Artist',
    artists: t.artist ? t.artist.split(/,\s*/).filter(Boolean) : ['Unknown Artist'],
    album: t.album || '',
    albumId: t.albumBrowseId || undefined,
    durationSec: Math.max(0, Number(t.durationSeconds) || 0),
    artworkUrl: t.artworkUrl || '',
    accentColor: '#7c9bff',
    qualityTier: 'high',
    sampleRateHz: 48000,
    bitDepth: 16,
    codec: 'OPUS',
    bitrateKbps: 256,
  };
}

export function mapYtAlbum(e: BackendYtEntity): Album {
  const id = e.browseId || e.playlistId || e.name;
  return {
    id,
    title: e.name || 'Unknown Album',
    artist: e.artist || e.subtitle || '',
    year: 0,
    type: 'Album',
    artworkUrl: e.artworkUrl || '',
    accentColor: '#7c9bff',
    trackIds: [],
  };
}

export function mapYtArtist(e: BackendYtEntity): Artist {
  const id = e.browseId || e.name;
  return {
    id,
    name: e.name || 'Unknown Artist',
    artworkUrl: e.artworkUrl || '',
    bannerUrl: e.artworkUrl || '',
    accentColor: '#7c9bff',
    monthlyListeners: e.subtitle || '',
    bio: '',
    topTrackIds: [],
    albumIds: [],
    isFollowing: false,
  };
}

function mapStoredTrack(t: BackendStoredTrack, idx: number): Track {
  const id = t.videoId || t.playbackUrl || `stored-${idx}-${t.name}`;
  return {
    id,
    videoId: t.videoId || '',
    title: t.name || 'Unknown Title',
    artist: t.artist || 'Unknown Artist',
    artists: t.artist ? t.artist.split(/,\s*/).filter(Boolean) : ['Unknown Artist'],
    album: t.album || '',
    durationSec: Math.max(0, Number(t.durationSeconds) || 0),
    artworkUrl: t.artworkUrl || '',
    accentColor: '#7c9bff',
    qualityTier: 'high',
    sampleRateHz: 48000,
    bitDepth: 16,
    codec: 'OPUS',
    bitrateKbps: 256,
    isLocal: Boolean(t.playbackUrl),
  };
}

class LibraryStore {
  tracks = $state<Track[]>([]);
  albums = $state<Album[]>([]);
  artists = $state<Artist[]>([]);
  playlists = $state<Playlist[]>([]);
  genres = $state<GenreItem[]>([]);
  likedIds = $state<string[]>([]);
  historyIds = $state<string[]>([]);
  searchHistory = $state<string[]>([]);
  localFolders = $state<string[]>([]);
  downloads = $state<DownloadItem[]>([]);
  homeShelves = $state<HomeShelfItem[]>([]);
  loadingHome = $state(false);
  homeError = $state<string | null>(null);

  upsertTracks(incoming: Track[]) {
    if (incoming.length === 0) return;
    const byId = new Map(this.tracks.map((t) => [t.id, t]));
    for (const track of incoming) {
      byId.set(track.id, track);
    }
    this.tracks = Array.from(byId.values());
  }

  upsertAlbums(incoming: Album[]) {
    if (incoming.length === 0) return;
    const byId = new Map(this.albums.map((a) => [a.id, a]));
    for (const album of incoming) {
      byId.set(album.id, album);
    }
    this.albums = Array.from(byId.values());
  }

  upsertArtists(incoming: Artist[]) {
    if (incoming.length === 0) return;
    const byId = new Map(this.artists.map((a) => [a.id, a]));
    for (const artist of incoming) {
      byId.set(artist.id, artist);
    }
    this.artists = Array.from(byId.values());
  }

  async connectBackend() {
    if (!isTauriRuntime()) return;
    await Promise.allSettled([
      this.loadLocalStorage(),
      this.loadHomeFeed(),
    ]);
  }

  async loadLocalStorage() {
    if (!isTauriRuntime()) return;
    const [history, folders, savedPlaylists, downloads] = await Promise.all([
      ipcInvoke<string[]>('library_search_history_load', { limit: 20 }),
      ipcInvoke<string[]>('library_local_folders'),
      ipcInvoke<BackendSavedPlaylist[]>('library_playlists_all'),
      ipcInvoke<BackendDownloadEntry[]>('library_downloads_all'),
    ]);

    if (Array.isArray(history)) this.searchHistory = history;
    if (Array.isArray(folders)) this.localFolders = folders;

    if (Array.isArray(savedPlaylists)) {
      const collectedTracks: Track[] = [];
      const mappedPlaylists: Playlist[] = [];
      const liked: string[] = [];

      for (const pl of savedPlaylists) {
        const plTracks = (pl.tracks ?? []).map((t, idx) => mapStoredTrack(t, idx));
        collectedTracks.push(...plTracks);
        const trackIds = plTracks.map((t) => t.id);
        if (pl.mode === 'liked') {
          liked.push(...trackIds);
        }
        mappedPlaylists.push({
          id: pl.mode === 'liked' ? 'pl-liked' : `pl-${pl.id}`,
          name: pl.title,
          description: pl.subtitle || '',
          artworkUrl: pl.customCoverUri || plTracks[0]?.artworkUrl || '',
          accentColor: '#7ca7db',
          trackIds,
          source: pl.mode === 'youtube' ? 'youtube' : 'local',
          owner: 'You',
          updatedAt: '',
          pinned: Boolean(pl.isPinned),
        });
      }

      this.upsertTracks(collectedTracks);
      this.playlists = mappedPlaylists;
      if (savedPlaylists.some((pl) => pl.mode === 'liked')) {
        this.likedIds = liked;
      }
    }

    if (Array.isArray(downloads)) {
      this.downloads = downloads
        .filter((d) => Boolean(d.videoId || d.trackKey))
        .map((d) => ({
          trackId: d.videoId || d.trackKey || '',
          status:
            d.status === 'completed' || d.status === 'downloading' || d.status === 'failed'
              ? d.status
              : 'queued',
          progress: typeof d.progress === 'number' ? d.progress : 0,
          sizeMb: typeof d.sizeBytes === 'number' ? d.sizeBytes / (1024 * 1024) : 0,
          format: 'FLAC / OPUS',
          localPath: d.localPath || '',
        }));
    }
  }

  private applyHomeShelves(shelves: BackendYtHomeShelf[]) {
    const mappedShelves: HomeShelfItem[] = [];
    const nextTracks: Track[] = [];
    const nextAlbums: Album[] = [];
    const nextArtists: Artist[] = [];

    for (const shelf of shelves) {
      const tracks = (shelf.tracks ?? []).map(mapYtTrack);
      const trackCards = (shelf.trackCards ?? []).map(mapYtTrack);
      const entities: HomeEntityItem[] = (shelf.entities ?? []).map((e) => {
        const rawKind = (e.kind || '').toLowerCase();
        const kind: HomeEntityItem['kind'] =
          rawKind === 'artist'
            ? 'artist'
            : rawKind === 'playlist'
              ? 'playlist'
              : rawKind === 'mix'
                ? 'mix'
                : 'album';
        if (kind === 'album') nextAlbums.push(mapYtAlbum(e));
        else if (kind === 'artist') nextArtists.push(mapYtArtist(e));
        return {
          kind,
          id: e.browseId || e.playlistId || e.name,
          name: e.name || '',
          subtitle: e.subtitle || e.artist || '',
          browseId: e.browseId || '',
          playlistId: e.playlistId || '',
          artworkUrl: e.artworkUrl || '',
        };
      });

      nextTracks.push(...tracks, ...trackCards);
      if (tracks.length > 0 || trackCards.length > 0 || entities.length > 0) {
        mappedShelves.push({
          title: shelf.title || 'Recommended',
          subtitle: shelf.subtitle || '',
          tracks,
          trackCards,
          entities,
        });
      }
    }

    this.homeShelves = mappedShelves;
    this.upsertTracks(nextTracks);
    this.upsertAlbums(nextAlbums);
    this.upsertArtists(nextArtists);
  }

  async loadHomeFeed(authenticated = true, forceFresh = false) {
    if (!isTauriRuntime()) return;
    this.loadingHome = true;
    this.homeError = null;
    try {
      if (!forceFresh && this.homeShelves.length === 0) {
        const cached = await ipcInvoke<BackendYtHomeShelf[]>('yt_swr_home_shelves', {
          forceFresh: false,
        });
        if (Array.isArray(cached) && cached.length > 0) {
          this.applyHomeShelves(cached);
        }
      }

      let shelves = await ipcInvoke<BackendYtHomeShelf[]>('yt_fetch_home_shelves', {
        maxShelves: 16,
        maxItemsPerShelf: 20,
        maxPages: 3,
        authenticated,
      });

      if ((!Array.isArray(shelves) || shelves.length === 0) && authenticated) {
        shelves = await ipcInvoke<BackendYtHomeShelf[]>('yt_fetch_home_shelves', {
          maxShelves: 16,
          maxItemsPerShelf: 20,
          maxPages: 2,
          authenticated: false,
        });
      }

      if (Array.isArray(shelves) && shelves.length > 0) {
        this.applyHomeShelves(shelves);
      }
    } catch (err) {
      this.homeError = err instanceof Error ? err.message : 'Failed to load YouTube Music feed';
    } finally {
      this.loadingHome = false;
    }
  }

  trackById(id: string): Track | undefined {
    return this.tracks.find((t) => t.id === id);
  }

  tracksByIds(ids: string[]): Track[] {
    return ids.map((id) => this.trackById(id)).filter((t): t is Track => Boolean(t));
  }

  isLiked(trackId: string): boolean {
    return this.likedIds.includes(trackId);
  }

  toggleLike(trackId: string) {
    const removing = this.likedIds.includes(trackId);
    if (removing) {
      this.likedIds = this.likedIds.filter((id) => id !== trackId);
    } else {
      this.likedIds = [trackId, ...this.likedIds];
    }
    const likedPl = this.playlists.find((p) => p.id === 'pl-liked');
    if (likedPl) likedPl.trackIds = [...this.likedIds];

    const track = this.trackById(trackId);
    if (track?.videoId) {
      void ipcInvoke(removing ? 'yt_remove_video_like' : 'yt_like_video', {
        videoId: track.videoId,
      });
    }
  }

  createPlaylist(name: string, description = ''): Playlist {
    const cleanName = name.trim() || 'Untitled Playlist';
    const id = `pl-${Date.now()}`;
    const pl: Playlist = {
      id,
      name: cleanName,
      description,
      artworkUrl: '',
      accentColor: '#7ca7db',
      trackIds: [],
      source: 'local',
      owner: 'You',
      updatedAt: 'Just now',
    };
    this.playlists = [pl, ...this.playlists];
    void ipcInvoke('library_playlists_create', { title: cleanName, subtitle: description });
    return pl;
  }

  renamePlaylist(id: string, name: string) {
    const pl = this.playlists.find((p) => p.id === id);
    if (pl && name.trim()) {
      pl.name = name.trim();
      pl.updatedAt = 'Just now';
      const numericId = Number(id.replace(/^pl-/, ''));
      if (Number.isFinite(numericId)) {
        void ipcInvoke('library_playlists_rename', { id: numericId, title: pl.name });
      }
    }
  }

  togglePinPlaylist(id: string) {
    const pl = this.playlists.find((p) => p.id === id);
    if (pl) {
      pl.pinned = !pl.pinned;
      const numericId = Number(id.replace(/^pl-/, ''));
      if (Number.isFinite(numericId)) {
        void ipcInvoke('library_playlists_set_pinned', { id: numericId, pinned: pl.pinned });
      }
    }
  }

  deletePlaylist(id: string) {
    if (id === 'pl-liked') return;
    this.playlists = this.playlists.filter((p) => p.id !== id);
    const numericId = Number(id.replace(/^pl-/, ''));
    if (Number.isFinite(numericId)) {
      void ipcInvoke('library_playlists_delete', { id: numericId });
    }
  }

  addTrackToPlaylist(playlistId: string, trackId: string) {
    const pl = this.playlists.find((p) => p.id === playlistId);
    if (pl && !pl.trackIds.includes(trackId)) {
      pl.trackIds = [...pl.trackIds, trackId];
      pl.updatedAt = 'Just now';
    }
  }

  removeTrackFromPlaylist(playlistId: string, trackId: string) {
    const pl = this.playlists.find((p) => p.id === playlistId);
    if (pl) {
      pl.trackIds = pl.trackIds.filter((id) => id !== trackId);
    }
  }

  addLocalFolder(path: string) {
    const clean = path.trim();
    if (clean && !this.localFolders.includes(clean)) {
      this.localFolders = [...this.localFolders, clean];
      void ipcInvoke('library_local_add_folder', { path: clean }).then(() =>
        ipcInvoke('library_local_rescan')
      );
    }
  }

  removeLocalFolder(path: string) {
    this.localFolders = this.localFolders.filter((f) => f !== path);
    void ipcInvoke('library_local_forget_folder', { path });
  }

  pushSearchHistory(query: string) {
    const q = query.trim();
    if (!q) return;
    this.searchHistory = [q, ...this.searchHistory.filter((item) => item.toLowerCase() !== q.toLowerCase())].slice(0, 12);
    void ipcInvoke('library_search_history_push', { query: q });
  }

  removeSearchHistory(query: string) {
    this.searchHistory = this.searchHistory.filter((q) => q !== query);
    void ipcInvoke('library_search_history_remove', { query });
  }

  clearSearchHistory() {
    this.searchHistory = [];
    void ipcInvoke('library_search_history_clear');
  }

  enqueueDownload(track: Track) {
    if (this.downloads.some((d) => d.trackId === track.id)) return;
    this.downloads = [
      {
        trackId: track.id,
        status: 'queued',
        progress: 0,
        sizeMb: 0,
        format: `${track.codec} ${track.bitDepth}-bit / ${(track.sampleRateHz / 1000).toFixed(1)} kHz`,
        localPath: '',
      },
      ...this.downloads,
    ];
    void ipcInvoke('library_downloads_enqueue', {
      args: {
        title: track.title,
        artist: track.artist,
        album: track.album,
        artworkUrl: track.artworkUrl,
        videoId: track.videoId,
      },
    });
  }

  removeDownload(trackId: string) {
    this.downloads = this.downloads.filter((d) => d.trackId !== trackId);
    void ipcInvoke('library_downloads_delete', { key: trackId });
  }

  toggleFollowArtist(artistId: string) {
    const artist = this.artists.find((a) => a.id === artistId);
    if (artist) artist.isFollowing = !artist.isFollowing;
  }
}

export const libraryStore = new LibraryStore();
