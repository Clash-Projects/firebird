import type { Album, Artist, DownloadItem, GenreItem, Playlist, Track } from './types';

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
    if (this.likedIds.includes(trackId)) {
      this.likedIds = this.likedIds.filter((id) => id !== trackId);
    } else {
      this.likedIds = [trackId, ...this.likedIds];
    }
    const likedPl = this.playlists.find((p) => p.id === 'pl-liked');
    if (likedPl) likedPl.trackIds = [...this.likedIds];
  }

  createPlaylist(name: string, description = ''): Playlist {
    const id = `pl-${Date.now()}`;
    const pl: Playlist = {
      id,
      name: name.trim() || 'Untitled Playlist',
      description,
      artworkUrl: '',
      accentColor: '#7ca7db',
      trackIds: [],
      source: 'local',
      owner: 'You',
      updatedAt: 'Just now',
    };
    this.playlists = [pl, ...this.playlists];
    return pl;
  }

  renamePlaylist(id: string, name: string) {
    const pl = this.playlists.find((p) => p.id === id);
    if (pl && name.trim()) {
      pl.name = name.trim();
      pl.updatedAt = 'Just now';
    }
  }

  togglePinPlaylist(id: string) {
    const pl = this.playlists.find((p) => p.id === id);
    if (pl) pl.pinned = !pl.pinned;
  }

  deletePlaylist(id: string) {
    if (id === 'pl-liked') return;
    this.playlists = this.playlists.filter((p) => p.id !== id);
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
    }
  }

  removeLocalFolder(path: string) {
    this.localFolders = this.localFolders.filter((f) => f !== path);
  }

  pushSearchHistory(query: string) {
    const q = query.trim();
    if (!q) return;
    this.searchHistory = [q, ...this.searchHistory.filter((item) => item.toLowerCase() !== q.toLowerCase())].slice(0, 12);
  }

  removeSearchHistory(query: string) {
    this.searchHistory = this.searchHistory.filter((q) => q !== query);
  }

  clearSearchHistory() {
    this.searchHistory = [];
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
  }

  removeDownload(trackId: string) {
    this.downloads = this.downloads.filter((d) => d.trackId !== trackId);
  }

  toggleFollowArtist(artistId: string) {
    const artist = this.artists.find((a) => a.id === artistId);
    if (artist) artist.isFollowing = !artist.isFollowing;
  }
}

export const libraryStore = new LibraryStore();
