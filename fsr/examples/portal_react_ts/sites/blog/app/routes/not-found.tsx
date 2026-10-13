export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <section className="space-y-3">
      <h1 className="font-serif text-3xl">No page at {params.path}</h1>
      <a href="/blog" className="text-violet-700 hover:underline">Back to the posts</a>
    </section>
  );
}
