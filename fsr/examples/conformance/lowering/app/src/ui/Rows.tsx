import { useState } from "react";

export default function Rows({ rows }: { rows: number[] }) {
  const [picked, setPicked] = useState(-1);
  const looped = [];
  for (const n of rows) {
    looped.push(
      <li key={n} className={n === picked ? "row picked" : "row"}>
        {new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(n)}
      </li>,
    );
  }
  const counted = [];
  for (let i = 0; i < rows.length; i++) {
    counted.push(<li key={i} className={rows[i] === picked ? "row picked" : "row"}>{new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(rows[i] * 2)}</li>);
  }
  return (
    <section className="rows">
      <ul className="looped">{looped}</ul>
      <ul className="counted">{counted}</ul>
      <ul className="mapped">
        {rows.map((n) => (
          <li key={n}>{new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(n)}</li>
        ))}
      </ul>
      <button className="pick" onClick={() => setPicked(rows[0])}>
        pick
      </button>
    </section>
  );
}
