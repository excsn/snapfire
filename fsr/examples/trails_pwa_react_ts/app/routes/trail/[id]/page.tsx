import type { TrailIdProps } from "@generated/client";

export default function Trail({ trail }: TrailIdProps) {
  return (
    <article>
      <h1>{trail.name}</h1>
      <p className="facts">
        {trail.region}: {trail.km} km, {trail.climb} m of climb
      </p>
      <ul className="notes">
        {trail.notes.map((note) => (
          <li key={note}>{note}</li>
        ))}
      </ul>
    </article>
  );
}
