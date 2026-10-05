import { useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import {
  addSource,
  getSources,
  sourceNameExists,
  type Source,
} from 'react-native-iptelly';
import { SourceType, errorMessage } from '../core';
import { useRemote } from '../remote';
import { colors, fonts, px } from '../theme';

type Kind = 'xtream' | 'm3u';

// Adding a playlist uses Android's own focus, which text fields need for
// the on-screen keyboard. `onDone` gets the new playlist, or nothing if the
// form was closed.
export function AddPlaylist({ onDone }: { onDone: (added?: Source) => void }) {
  const [kind, setKind] = useState<Kind>('xtream');
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [epgUrl, setEpgUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useRemote(key => {
    if (key === 'back' && !busy) {
      onDone();
    }
    return true;
  });

  const save = async () => {
    const trimmed = name.trim();
    if (!trimmed || !url.trim()) {
      setError('Enter a name and a URL.');
      return;
    }
    if (kind === 'xtream' && (!username || !password)) {
      setError('Enter the username and password.');
      return;
    }
    setBusy(true);
    setError(undefined);
    try {
      if (await sourceNameExists(trimmed)) {
        throw new Error('There is already a playlist with that name.');
      }
      await addSource({
        name: trimmed,
        url: url.trim(),
        username: kind === 'xtream' ? username : undefined,
        password: kind === 'xtream' ? password : undefined,
        sourceType: kind === 'xtream' ? SourceType.XTREAM : SourceType.M3U_LINK,
        epgUrl: kind === 'm3u' && epgUrl.trim() ? epgUrl.trim() : undefined,
        enabled: true,
      });
      const sources = await getSources();
      onDone(sources.find(s => s.name === trimmed));
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  const field = (
    label: string,
    value: string,
    onChange: (text: string) => void,
    secure = false,
  ) => (
    <TextInput
      placeholder={label}
      placeholderTextColor={colors.textDim}
      value={value}
      onChangeText={onChange}
      secureTextEntry={secure}
      autoCapitalize="none"
      autoCorrect={false}
      editable={!busy}
      style={styles.input}
    />
  );

  return (
    <View style={styles.form}>
      <View style={styles.kinds}>
        <Choice
          label="Xtream"
          chosen={kind === 'xtream'}
          onPress={() => setKind('xtream')}
          preferred
        />
        <Choice
          label="M3U link"
          chosen={kind === 'm3u'}
          onPress={() => setKind('m3u')}
        />
      </View>
      {field('Name', name, setName)}
      {field(
        kind === 'xtream' ? 'Server, e.g. http://host:port' : 'Playlist URL',
        url,
        setUrl,
      )}
      {kind === 'xtream' ? (
        <>
          {field('Username', username, setUsername)}
          {field('Password', password, setPassword, true)}
        </>
      ) : (
        field('Guide (XMLTV) URL, optional', epgUrl, setEpgUrl)
      )}
      <Choice label={busy ? 'Adding…' : 'Add playlist'} onPress={save} />
      {error != null && <Text style={styles.error}>{error}</Text>}
    </View>
  );
}

export function Choice({
  label,
  chosen,
  onPress,
  preferred,
}: {
  label: string;
  chosen?: boolean;
  onPress: () => void;
  preferred?: boolean;
}) {
  return (
    <Pressable
      onPress={onPress}
      hasTVPreferredFocus={preferred}
      style={({ focused }) => [
        styles.button,
        chosen && styles.chosen,
        focused && styles.focused,
      ]}
    >
      {({ focused }) => (
        <Text
          style={[styles.buttonText, focused && { color: colors.textDark }]}
        >
          {label}
        </Text>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  form: {
    paddingHorizontal: px(50),
    paddingTop: px(24),
    gap: px(18),
  },
  kinds: {
    flexDirection: 'row',
    gap: px(18),
  },
  input: {
    height: px(76),
    paddingHorizontal: px(24),
    borderRadius: px(10),
    backgroundColor: 'rgba(0, 0, 0, 0.18)',
    color: colors.text,
    fontSize: fonts.normal,
  },
  button: {
    height: px(76),
    paddingHorizontal: px(34),
    justifyContent: 'center',
    borderRadius: px(10),
    backgroundColor: 'rgba(0, 0, 0, 0.12)',
  },
  chosen: {
    backgroundColor: colors.pillSoft,
  },
  focused: {
    backgroundColor: colors.pill,
  },
  buttonText: {
    color: colors.text,
    fontSize: fonts.normal,
  },
  error: {
    color: colors.textDark,
    fontSize: fonts.small,
    backgroundColor: '#ffd6d1',
    padding: px(16),
    borderRadius: px(10),
  },
});
