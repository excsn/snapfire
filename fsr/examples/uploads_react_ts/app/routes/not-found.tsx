export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <main className="page">
      <h1>No page at {params.path}</h1>
      <a href="/">Back</a>
    </main>
  );
}
