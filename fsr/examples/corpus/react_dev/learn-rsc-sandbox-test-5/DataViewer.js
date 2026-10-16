'use client';

export default function DataViewer({ map, set, date, big }) {
  const checks = [
    ['Map', map instanceof Map, () => (
      <ul>{[...map.entries()].map(([k, v]) => <li key={k}>{k}: {v}</li>)}</ul>
    )],
    ['Set', set instanceof Set, () => (
      <ul>{[...set].map(v => <li key={v}>{v}</li>)}</ul>
    )],
    ['Date', date instanceof Date, () => (
      <p>{date.toISOString()}</p>
    )],
    ['BigInt', typeof big === 'bigint', () => (
      <p>{big.toString()}</p>
    )],
  ];

  return (
    <div>
      {checks.map(([label, passed, render]) => (
        <div key={label} style={{ marginBottom: 12 }}>
          <strong>{label}: {passed ? 'pass' : 'FAIL'}</strong>
          {render()}
        </div>
      ))}
    </div>
  );
}
