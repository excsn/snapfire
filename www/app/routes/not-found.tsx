export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <div className="page">
      <section className="hero hero-small">
        <h1 className="hero-title">No page at {params.path}</h1>
        <p className="hero-lede">The link may be old, or the address mistyped.</p>
        <div className="hero-cta">
          <a className="btn btn-primary" href="/">
            Back to the start
          </a>
          <a className="btn btn-secondary" href="/fsr/docs">
            The FSR guide
          </a>
        </div>
      </section>
    </div>
  );
}
