export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <section className="hero">
      <h1>No page at {params.path}</h1>
      <a href="{{home}}">Back to the start</a>
    </section>
  );
}
