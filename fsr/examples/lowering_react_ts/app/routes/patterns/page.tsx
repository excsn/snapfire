import { Island } from "@snapfire/fsr-authoring/template";

import Line from "@src/ui/Line";

interface OrderLine {
  sku: string;
  qty: number;
  price?: number;
}

export default function Patterns(props: { id: string; name: string; city: string; country: string; head: string; count: number; lines: OrderLine[] }) {
  const { id, name, city, country, lines } = props;
  const [first, , third] = lines;
  return (
    <section className="patterns">
      <p className="who">{id} for {name} in {city}, {country}</p>
      <p className="ends">{first.sku} {third.sku}</p>
      <ul className="lines">
        {lines.map(({ sku, qty, price = 0 }) => {
          const [total, label] = [qty * price, sku.toUpperCase()];
          return (
            <li key={sku}>
              {label} {total}
            </li>
          );
        })}
      </ul>
      <Island>
        <Line line={first} />
      </Island>
    </section>
  );
}
