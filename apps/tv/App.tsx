/**
 * IPTelly for TVs. For now this only starts the Rust core and lists the
 * sources, to show the app and the core are talking.
 *
 * @format
 */

import { useCallback, useEffect, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import {
  CachesDirectoryPath,
  DocumentDirectoryPath,
} from '@dr.pogodin/react-native-fs';
import { getSources, init, type Source } from 'react-native-iptelly';

// The core keeps its database in the data folder and its logs and
// downloaded playlists in the cache folder. Android gives every app its own.
const ready = init(DocumentDirectoryPath, CachesDirectoryPath);

function App() {
  const [sources, setSources] = useState<Source[]>();
  const [error, setError] = useState<string>();

  const load = useCallback(async () => {
    setError(undefined);
    try {
      await ready;
      setSources(await getSources());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <View style={styles.screen}>
      <Text style={styles.title}>IPTelly</Text>
      {error != null ? (
        <Text style={styles.error}>{error}</Text>
      ) : sources == null ? (
        <Text style={styles.text}>Starting…</Text>
      ) : sources.length === 0 ? (
        <Text style={styles.text}>No sources yet.</Text>
      ) : (
        sources.map(source => (
          <Text key={String(source.id)} style={styles.text}>
            {source.name}
          </Text>
        ))
      )}
      <Pressable
        onPress={load}
        hasTVPreferredFocus
        style={({ focused }) => [styles.button, focused && styles.focused]}
      >
        <Text style={styles.text}>Reload</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    padding: 48,
    gap: 16,
    backgroundColor: '#111',
  },
  title: {
    color: '#fff',
    fontSize: 40,
    fontWeight: 'bold',
  },
  text: {
    color: '#ddd',
    fontSize: 24,
  },
  error: {
    color: '#f66',
    fontSize: 24,
  },
  button: {
    alignSelf: 'flex-start',
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 8,
    borderWidth: 2,
    borderColor: '#444',
  },
  focused: {
    borderColor: '#fff',
    backgroundColor: '#333',
  },
});

export default App;
