export default function Failed({ error, kind }: { error: string; kind: string }) {
  return (
    <section className="failed">
      <p className="why">
        {kind}: {error}
      </p>
    </section>
  );
}
