export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <section className="page">
      <h1>No page at {params.path}</h1>
      <a href="/blog">Back to the posts</a>
    </section>
  );
}
