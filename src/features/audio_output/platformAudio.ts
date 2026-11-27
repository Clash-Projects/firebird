export type HostPlatform = 'windows' | 'macos' | 'linux';

export interface PlatformAudioCopy {
  driverName: string;
  exclusiveTitle: string;
  exclusiveDescription: string;
  exclusiveOffReason: string;
  mediaControlsTitle: string;
}

export function detectHostPlatform(): HostPlatform {
  if (typeof navigator === 'undefined') return 'windows';
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac os') || ua.includes('macintosh')) return 'macos';
  if (ua.includes('linux') && !ua.includes('android')) return 'linux';
  return 'windows';
}

export function getPlatformAudioCopy(platform: HostPlatform): PlatformAudioCopy {
  if (platform === 'macos') {
    return {
      driverName: 'CoreAudio',
      exclusiveTitle: 'CoreAudio Hog Mode',
      exclusiveDescription: 'Claim exclusive DAC access and switch hardware sample rate',
      exclusiveOffReason: 'CoreAudio Hog mode is off',
      mediaControlsTitle: 'macOS Media Controls (Now Playing)',
    };
  }
  if (platform === 'linux') {
    return {
      driverName: 'ALSA / PipeWire',
      exclusiveTitle: 'Direct Hardware Output (ALSA / PipeWire)',
      exclusiveDescription: 'Bypass desktop mixer for native sample-rate bit-perfect output',
      exclusiveOffReason: 'Direct bit-perfect output is off',
      mediaControlsTitle: 'Linux Media Controls (MPRIS)',
    };
  }
  return {
    driverName: 'WASAPI',
    exclusiveTitle: 'WASAPI Exclusive Mode',
    exclusiveDescription: 'Bypass Windows mixer for direct bit-perfect DAC output',
    exclusiveOffReason: 'WASAPI Exclusive mode is off',
    mediaControlsTitle: 'Windows Media Controls (SMTC)',
  };
}
