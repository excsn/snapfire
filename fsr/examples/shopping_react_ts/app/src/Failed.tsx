export default function Failed({ error }: { error: string }) {
  return (
    <main className="failed">
      <h1>That did not load</h1>
      <p className="reason">{error}</p>
      <p>
        <a href="/">back to the catalog</a>
      </p>
    </main>
  );
}
