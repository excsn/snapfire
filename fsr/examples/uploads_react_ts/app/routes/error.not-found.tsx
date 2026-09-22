export default function Missing({ error }: { error: string }) {
  return (
    <main className="page">
      <h1>Nothing here</h1>
      <p className="error">{error}</p>
      <a href="/">Back</a>
    </main>
  );
}
