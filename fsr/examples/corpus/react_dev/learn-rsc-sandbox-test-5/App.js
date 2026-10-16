import DataViewer from './DataViewer';

export default function App() {
  const map = new Map([
    ['alice', 100],
    ['bob', 200],
  ]);
  const set = new Set(['react', 'next', 'remix']);
  const date = new Date('2025-06-15T12:00:00Z');
  const big = 9007199254740993n;

  return (
    <div>
      <h1>Flight Data Types</h1>
      <DataViewer map={map} set={set} date={date} big={big} />
    </div>
  );
}
