'use client';
import { use } from 'react';

function now() {
  return Date.now();
}
export default function UserCard({ userPromise, serverTime }) {
  const user = use(userPromise);
  const elapsed = now() - serverTime;
  return (
    <div style={{
      border: '1px solid #ccc',
      borderRadius: 8,
      padding: 16,
    }}>
      <strong>{user.name}</strong>
      <p>{user.role}</p>
      <p style={{ fontSize: 13 }}>
        Rendered {elapsed}ms after server created the promise.
      </p>
      <p style={{ color: '#666', fontSize: 12 }}>
        ~3000ms = promise already resolved, waited only for parent.
        ~5000ms would mean the promise restarted on the client.
      </p>
    </div>
  );
}
