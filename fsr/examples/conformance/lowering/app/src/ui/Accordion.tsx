import { useState, type ReactNode } from "react";

export default function Accordion({ label, items }: { label: ReactNode; items: { id: string; q: string; body: ReactNode }[] }) {
  const [open, setOpen] = useState(0);
  return (
    <section className="accordion">
      <h2 className="label">{label}</h2>
      {items.map((item, i) => (
        <div key={item.id} className="item">
          <button className="toggle" onClick={() => setOpen(i)}>
            {item.q}
          </button>
          {open === i && <div className="body">{item.body}</div>}
        </div>
      ))}
    </section>
  );
}
