export default function Failed({ error }: { error: string }) {
  return (
    <div className="status-page">
      <h1>Status did not load</h1>
      <p>{error}</p>
    </div>
  );
}
