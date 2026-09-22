export default function Failed({ error, kind }: { error: string; kind: string }) {
  return (
    <main className="page">
      <h1>That did not load</h1>
      <p className="error">
        {kind}: {error}
      </p>
      <a href="/">Back</a>
    </main>
  );
}
