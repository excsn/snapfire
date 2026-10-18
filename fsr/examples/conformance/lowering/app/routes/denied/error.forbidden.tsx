export default function Forbidden({ error, kind }: { error: string; kind: string }) {
  return (
    <section className="forbidden">
      <h1>Not for you</h1>
      <p className="why">
        {kind}: {error}
      </p>
    </section>
  );
}
