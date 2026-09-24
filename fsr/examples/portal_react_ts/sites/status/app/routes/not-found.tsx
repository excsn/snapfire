export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <div className="status-page">
      <h1>No page at {params.path}</h1>
      <a href="/status">Back to the status page</a>
    </div>
  );
}
