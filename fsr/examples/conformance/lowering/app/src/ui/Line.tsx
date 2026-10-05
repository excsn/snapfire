import { useState } from "react";

export default function Line({ line: { sku, qty, price = 0 }, unit = "each" }: { line: { sku: string; qty: number; price?: number }; unit?: string }) {
  const [extra, setExtra] = useState(0);
  const [count, cost] = [qty + extra, (qty + extra) * price];
  return (
    <div className="line">
      <p className="summary">{count} {sku} at {price} {unit} = {cost}</p>
      <button className="more" onClick={() => setExtra(extra + 1)}>
        more
      </button>
    </div>
  );
}
