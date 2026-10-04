/**
 * IPTelly for TVs.
 *
 * @format
 */

import { StatusBar } from 'react-native';
import { Home } from './src/Home';

function App() {
  return (
    <>
      <StatusBar hidden />
      <Home />
    </>
  );
}

export default App;
