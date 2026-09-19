export default function Failed({ error }: { error: string }) {
  return (
    <section className="page">
      <h1>That did not load</h1>
      <p>{error}</p>
      <a href="/blog">Back to the posts</a>
    </section>
  );
}
