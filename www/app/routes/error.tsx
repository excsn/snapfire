export default function Failed({ error }: { error: string }) {
  return (
    <div className="page">
      <section className="hero hero-small">
        <h1 className="hero-title">That did not load</h1>
        <p className="hero-lede">{error}</p>
        <div className="hero-cta">
          <a className="btn btn-primary" href="/">
            Back to the start
          </a>
        </div>
      </section>
    </div>
  );
}
