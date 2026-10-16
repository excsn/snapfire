import { Suspense } from 'react';
import SlowData from './SlowData';
import Timestamp from './Timestamp';

export default function App() {
  return (
    <div>
      <h1>Streaming Proof</h1>
      <p>Shell rendered at: <Timestamp /></p>
      <Suspense fallback={<p>⏳ Waiting for data to stream in...</p>}>
        <SlowData />
      </Suspense>
    </div>
  );
}
