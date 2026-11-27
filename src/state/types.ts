export type RepeatMode = 'off' | 'all' | 'one';
export type AudioQualityTier = 'dataSaver' | 'high' | 'cdFlac' | 'hiRes96' | 'hiRes192' | 'atmos';
export type NowPlayingStyle = 'standard' | 'minimal' | 'fullBleed';

export interface Track {
  id: string;
  videoId: string;
  title: string;
  artist: string;
  artists: string[];
  album: string;
  albumId?: string;
  durationSec: number;
  artworkUrl: string;
  accentColor: string;
  qualityTier: AudioQualityTier;
  sampleRateHz: number;
  bitDepth: number;
  codec: string;
  bitrateKbps: number;
  isExplicit?: boolean;
  isLocal?: boolean;
  isDownloaded?: boolean;
  bpm?: number;
  key?: string;
  canvasUrl?: string;
  playedAt?: string;
}

export interface Album {
  id: string;
  title: string;
  artist: string;
  year: number;
  artworkUrl: string;
  accentColor: string;
  trackIds: string[];
  type: 'Album' | 'EP' | 'Single';
}

export interface Artist {
  id: string;
  name: string;
  monthlyListeners: string;
  artworkUrl: string;
  bannerUrl: string;
  accentColor: string;
  bio: string;
  topTrackIds: string[];
  albumIds: string[];
  isFollowing?: boolean;
}

export interface Playlist {
  id: string;
  name: string;
  description: string;
  artworkUrl: string;
  accentColor: string;
  trackIds: string[];
  source: 'local' | 'youtube' | 'spotify' | 'generated';
  pinned?: boolean;
  owner: string;
  updatedAt: string;
}

export interface LyricWord {
  text: string;
  startMs: number;
  endMs: number;
}

export interface LyricLine {
  startMs: number;
  endMs: number;
  text: string;
  translation?: string;
  romanized?: string;
  words?: LyricWord[];
}

export interface LyricsDocument {
  trackId: string;
  provider: string;
  synced: boolean;
  wordSynced: boolean;
  lines: LyricLine[];
}

export interface PodcastEpisode {
  id: string;
  showId: string;
  title: string;
  description: string;
  publishedAt: string;
  durationSec: number;
  resumeSec: number;
  artworkUrl: string;
}

export interface PodcastShow {
  id: string;
  title: string;
  publisher: string;
  artworkUrl: string;
  description: string;
  episodes: PodcastEpisode[];
}

export interface GenreItem {
  id: string;
  name: string;
  color: string;
  subtitle: string;
  trackIds: string[];
}

export interface AudioDevice {
  id: string;
  name: string;
  driver: 'WASAPI' | 'CoreAudio' | 'PipeWire';
  maxSampleRateHz: number;
  maxBitDepth: number;
  supportsExclusive: boolean;
  isDefault: boolean;
}

export interface DownloadItem {
  trackId: string;
  status: 'queued' | 'downloading' | 'completed' | 'failed';
  progress: number;
  sizeMb: number;
  format: string;
  localPath: string;
}

export interface ToastMessage {
  id: string;
  title: string;
  subtitle?: string;
  severity: 'info' | 'success' | 'warning' | 'error';
  actionLabel?: string;
  onAction?: () => void;
}
