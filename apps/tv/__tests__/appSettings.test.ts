import {
  DEFAULT_SETTINGS,
  GENERAL_ITEMS,
  generalList,
  streamUrl,
} from '../src/appSettings';

jest.mock('@dr.pogodin/react-native-fs', () => ({}));

test('multicast streams go through the UDP proxy', () => {
  expect(streamUrl('udp://@239.1.1.1:1234', '192.168.1.2:4022')).toBe(
    'http://192.168.1.2:4022/udp/239.1.1.1:1234',
  );
  expect(streamUrl('rtp://239.1.1.1:5000', 'http://proxy:4022/')).toBe(
    'http://proxy:4022/rtp/239.1.1.1:5000',
  );
});

test('other streams, or no proxy, are left alone', () => {
  expect(streamUrl('http://example.com/1.m3u8', 'proxy:4022')).toBe(
    'http://example.com/1.m3u8',
  );
  expect(streamUrl('udp://@239.1.1.1:1234', '  ')).toBe(
    'udp://@239.1.1.1:1234',
  );
});

test('the General list shows switches and values', () => {
  const { toggles, details } = generalList({
    ...DEFAULT_SETTINGS,
    userAgent: 'TiviMate/4.7',
  });
  const at = (label: string) => GENERAL_ITEMS.findIndex(i => i.label === label);
  expect(toggles[at('Turn on last channel on app start')]).toBe(true);
  expect(toggles[at('Auto start app on boot')]).toBe(false);
  expect(toggles[at('User-Agent')]).toBeUndefined();
  expect(details[at('User-Agent')]).toBe('TiviMate/4.7');
  expect(details[at('UDP proxy (address:port)')]).toBe('Not set');
  expect(details[at('Auto start app on wake up from sleep mode')]).toBe(
    'May not work on all devices',
  );
});
