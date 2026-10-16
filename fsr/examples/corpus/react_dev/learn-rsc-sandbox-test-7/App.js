import { testTypes, getResults } from './actions';
import TestButton from './TestButton';

export default async function App() {
  const results = await getResults();
  return (
    <div>
      <h1>Flight Types in Server Actions</h1>
      <TestButton testTypes={testTypes} />
      {results ? (
        <div>
          {results.map(r => (
            <div key={r.label} style={{ marginBottom: 12 }}>
              <strong>{r.label}: {r.ok ? 'pass' : 'FAIL'}</strong>
              <p>{r.detail}</p>
            </div>
          ))}
        </div>
      ) : (
        <p>Click the button to send typed data to the server action.</p>
      )}
    </div>
  );
}
