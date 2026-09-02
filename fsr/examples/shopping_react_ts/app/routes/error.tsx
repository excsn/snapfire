export default function Failed({ error }: { error: string }) {
  return (
    <main className="page failed">
      <section className="empty">
        <p className="empty-glyph">⚠️</p>
        <h1>That did not load</h1>
        <p className="reason">{error}</p>
        <a className="btn btn-primary" href="/">
          Back to the catalog
        </a>
      </section>
    </main>
  );
}
