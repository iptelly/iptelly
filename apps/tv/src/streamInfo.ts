// The resolution, frame rate and audio of the channel playing, for the
// boxes along the bottom of the info bar. The player reports them as it
// learns them, so any can be missing for a moment after a channel starts.

export type StreamInfo = {
  width?: number;
  height?: number;
  frameRate?: number;
  channels?: number;
};

// What the patched react-native-video adds to its tracks (see
// .yarn/patches): frameRate on video tracks, channelCount on audio tracks.
type VideoTrack = {
  width?: number;
  height?: number;
  frameRate?: number;
  selected?: boolean;
};
type AudioTrack = { channelCount?: number; selected?: boolean };

const chosen = <T extends { selected?: boolean }>(tracks: T[] = []) =>
  tracks.find(t => t.selected) ?? tracks[0];

export function fromVideoTracks(
  info: StreamInfo,
  tracks?: VideoTrack[],
): StreamInfo {
  // A stream with several versions (HLS) doesn't say which is playing,
  // so the one the size of the picture.
  const track =
    tracks?.find(t => t.selected) ??
    tracks?.find(t => t.width === info.width && t.height === info.height) ??
    tracks?.[0];
  return {
    ...info,
    width: info.width || track?.width || undefined,
    height: info.height || track?.height || undefined,
    frameRate: track?.frameRate || info.frameRate,
  };
}

export function fromAudioTracks(
  info: StreamInfo,
  tracks?: AudioTrack[],
): StreamInfo {
  return { ...info, channels: chosen(tracks)?.channelCount || info.channels };
}

// 4K, HD (1080), SHD (720) or SD. The width counts too, so a letterboxed
// film (1920×800) is still HD.
export function resolutionText({ width = 0, height = 0 }: StreamInfo) {
  if (width === 0 && height === 0) {
    return undefined;
  }
  if (width >= 3200 || height >= 2000) {
    return '4K';
  }
  if (width >= 1800 || height >= 1000) {
    return 'HD';
  }
  if (width >= 1200 || height >= 700) {
    return 'SHD';
  }
  return 'SD';
}

// Rounded, so 23.976 reads as 24 fps and 59.94 as 60.
export function frameRateText({ frameRate }: StreamInfo) {
  return frameRate ? `${Math.round(frameRate)} fps` : undefined;
}

export function audioText({ channels }: StreamInfo) {
  switch (channels) {
    case undefined:
      return undefined;
    case 1:
      return 'Mono';
    case 2:
      return 'Stereo';
    case 6:
      return '5.1';
    case 8:
      return '7.1';
    default:
      return `${channels} channels`;
  }
}
