export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <main className="page failed">
      <section className="empty">
        <p className="empty-glyph">🧭</p>
        <h1>No page at {params.path}</h1>
        <p>The link may be old, or the address mistyped.</p>
        <a className="btn btn-primary" href="/">
          Back to the catalog
        </a>
      </section>
    </main>
  );
}
