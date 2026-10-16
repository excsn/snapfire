import { Suspense } from 'react';
import Albums from './Albums';

export default function App() {
  return (
    <div>
      <h1>Music</h1>
      <Suspense fallback={<p>Loading albums...</p>}>
        <Albums />
      </Suspense>
    </div>
  );
}
