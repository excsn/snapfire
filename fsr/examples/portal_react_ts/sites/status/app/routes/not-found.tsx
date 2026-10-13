export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <div>
      <h1>No page at {params.path}</h1>
      <a href="/status" className="text-emerald-700">Back to the status page</a>
    </div>
  );
}
