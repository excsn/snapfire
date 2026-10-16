'use client';

export default function Timestamp() {
  return <strong>{new Date().toLocaleTimeString()}</strong>;
}
