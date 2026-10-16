'use client';
import { useTransition } from 'react';

export default function TestButton({ testTypes }) {
  const [pending, startTransition] = useTransition();

  function handleClick() {
    startTransition(async () => {
      await testTypes(
        new Map([['alice', 100], ['bob', 200]]),
        new Set(['react', 'next', 'remix']),
        new Date('2025-06-15T12:00:00Z'),
        9007199254740993n
      );
    });
  }

  return (
    <button onClick={handleClick} disabled={pending}>
      {pending ? 'Sending...' : 'Send typed data to server'}
    </button>
  );
}
