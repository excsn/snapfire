import { Suspense } from 'react';
import SlowParent from './SlowParent';
import UserCard from './UserCard';

async function fetchUser() {
  await new Promise(resolve => setTimeout(resolve, 2000));
  return { name: 'Alice', role: 'Engineer' };
}

function now() {
  return Date.now();
}

export default function App() {
  const serverTime = now();
  const userPromise = fetchUser();
  return (
    <div>
      <h1>Promise Streaming</h1>
      <p>Promise resolves in 2s. Parent suspends for 3s.</p>
      <Suspense fallback={<p>Outer: waiting for parent (3s)...</p>}>
        <SlowParent>
          <Suspense fallback={<p>Inner: waiting for data (should not appear!)</p>}>
            <UserCard userPromise={userPromise} serverTime={serverTime} />
          </Suspense>
        </SlowParent>
      </Suspense>
    </div>
  );
}
