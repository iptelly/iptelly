import {
  audioText,
  fromAudioTracks,
  fromVideoTracks,
  frameRateText,
  resolutionText,
} from '../src/streamInfo';

test('resolution reads as 4K, HD, SHD or SD', () => {
  expect(resolutionText({ width: 3840, height: 2160 })).toBe('4K');
  expect(resolutionText({ width: 1920, height: 1080 })).toBe('HD');
  // A letterboxed film is still HD.
  expect(resolutionText({ width: 1920, height: 800 })).toBe('HD');
  expect(resolutionText({ width: 1280, height: 720 })).toBe('SHD');
  expect(resolutionText({ width: 720, height: 576 })).toBe('SD');
  expect(resolutionText({})).toBeUndefined();
});

test('frame rates are rounded', () => {
  expect(frameRateText({ frameRate: 50 })).toBe('50 fps');
  expect(frameRateText({ frameRate: 25 })).toBe('25 fps');
  expect(frameRateText({ frameRate: 23.976 })).toBe('24 fps');
  expect(frameRateText({})).toBeUndefined();
});

test('audio reads as mono, stereo or surround', () => {
  expect(audioText({ channels: 1 })).toBe('Mono');
  expect(audioText({ channels: 2 })).toBe('Stereo');
  expect(audioText({ channels: 6 })).toBe('5.1');
  expect(audioText({ channels: 8 })).toBe('7.1');
  expect(audioText({})).toBeUndefined();
});

test('the frame rate measured later is added to what was known', () => {
  const loaded = fromAudioTracks(
    fromVideoTracks({ width: 1920, height: 1080 }, [{ width: 1920 }]),
    [{ channelCount: 2 }, { channelCount: 6, selected: true }],
  );
  expect(loaded).toEqual({ width: 1920, height: 1080, channels: 6 });
  expect(fromVideoTracks(loaded, [{ frameRate: 50 }]).frameRate).toBe(50);
});

test("a stream's frame rate is the playing version's", () => {
  const versions = [
    { width: 640, height: 360, frameRate: 30 },
    { width: 1920, height: 1080, frameRate: 60 },
  ];
  expect(
    fromVideoTracks({ width: 1920, height: 1080 }, versions).frameRate,
  ).toBe(60);
});
