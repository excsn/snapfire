export default function Failed({ error }: { error: string }) {
  return (
    <section className="space-y-3">
      <h1 className="font-serif text-3xl">That did not load</h1>
      <p>{error}</p>
      <a href="/blog" className="text-violet-700 hover:underline">Back to the posts</a>
    </section>
  );
}
