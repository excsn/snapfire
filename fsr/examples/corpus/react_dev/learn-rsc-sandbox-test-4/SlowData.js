import Timestamp from './Timestamp';

async function fetchData() {
  await new Promise(resolve => setTimeout(resolve, 2000));
  return ['Chunk A', 'Chunk B', 'Chunk C'];
}

export default async function SlowData() {
  const items = await fetchData();
  return (
    <div>
      <p>Data streamed in at: <Timestamp /></p>
      <ul>
        {items.map(item => (
          <li key={item}>{item}</li>
        ))}
      </ul>
    </div>
  );
}
