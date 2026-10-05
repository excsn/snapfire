import { useState } from "react";

export default function Shares({ items }: { items: { sku: string; left: number }[] }) {
  const [open, setOpen] = useState("");
  const total = items.reduce((sum, item) => sum + item.left, 0);
  const bar = (left: number, hot: boolean) => <span className={hot ? "bar hot" : "bar"} data-share={Math.round((left / total) * 100)} />;
  function label(sku: string, shown: boolean) {
    if (!shown) return <span className="sku">{sku}</span>;
    const upper = sku.toUpperCase();
    return <b className="sku">{upper}</b>;
  }
  return (
    <ol className="shares">
      {items.map((item) => {
        let note = "";
        if (item.left === 0) {
          note = "out";
        } else if (item.left < 3) {
          note = "low";
        }
        const shown = open === item.sku;
        return (
          <li key={item.sku} className="share">
            <button className="pick" onClick={() => setOpen(shown ? "" : item.sku)}>
              {label(item.sku, shown)}
            </button>
            {bar(item.left, shown)}
            {note && <i className="note">{note}</i>}
          </li>
        );
      })}
    </ol>
  );
}
