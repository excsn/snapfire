import { Link } from "@snapfire/fsr/components";
import type { RootProps } from "@generated/client";

const SHELVES = [
  { id: "all", label: "All" },
  { id: "to-read", label: "To read" },
  { id: "reading", label: "Reading" },
  { id: "done", label: "Done" },
];

export default function Shelf({ shelf, rows }: RootProps) {
  return (
    <section className="list">
      <nav className="shelves">
        {SHELVES.map((s) => (
          <Link key={s.id} href={s.id === "all" ? "/" : `/?shelf=${s.id}`} className={s.id === shelf ? "shelf on" : "shelf"}>
            {s.label}
          </Link>
        ))}
      </nav>
      {rows.length === 0 ? <p className="empty">Nothing on this shelf.</p> : null}
      <ul className="books">
        {rows.map((row) => (
          <li key={row.id} className={`book ${row.place}`}>
            <Link href={`/book/${row.id}`} className="title">
              {row.title}
            </Link>
            <span className="author">{row.author}</span>
            <span className="read">
              {row.read} / {row.pages}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
