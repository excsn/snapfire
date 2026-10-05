export default function Failed({ error }: { error: string }) {
  return (
    <section className="hero">
      <h1>That did not load</h1>
      <p>{error}</p>
      <a href="/">Back to the start</a>
    </section>
  );
}
