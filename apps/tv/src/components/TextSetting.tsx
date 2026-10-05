import { useState } from 'react';
import { StyleSheet, Text, TextInput, View } from 'react-native';
import { useRemote } from '../remote';
import { colors, fonts, px } from '../theme';
import { Choice } from './AddPlaylist';

// Typing one setting, such as the User-Agent, with Android's own focus for
// the on-screen keyboard. `onDone` gets the new value (empty to clear it),
// or nothing if Back closed the form.
export function TextSetting({
  value,
  placeholder,
  hint,
  onDone,
}: {
  value: string;
  placeholder: string;
  hint?: string;
  onDone: (value?: string) => void;
}) {
  const [text, setText] = useState(value);

  useRemote(key => {
    if (key === 'back') {
      onDone();
    }
    return true;
  });

  return (
    <View style={styles.form}>
      <TextInput
        value={text}
        onChangeText={setText}
        placeholder={placeholder}
        placeholderTextColor={colors.textDim}
        autoCapitalize="none"
        autoCorrect={false}
        hasTVPreferredFocus
        onSubmitEditing={() => onDone(text.trim())}
        style={styles.input}
      />
      {hint != null && <Text style={styles.hint}>{hint}</Text>}
      <View style={styles.buttons}>
        <Choice label="Save" onPress={() => onDone(text.trim())} />
        <Choice label="Clear" onPress={() => onDone('')} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  form: {
    paddingHorizontal: px(50),
    paddingTop: px(24),
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
  hint: {
    color: colors.text,
    fontSize: fonts.small,
  },
  buttons: {
    flexDirection: 'row',
    gap: px(18),
  },
});
