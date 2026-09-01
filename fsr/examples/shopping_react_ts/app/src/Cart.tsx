import { action } from "@snapfire/fsr-client";

interface Line {
  id: bigint | number;
  name: string;
  price_cents: bigint | number;
  quantity: bigint | number;
}

const remove = action("add_to_cart");
const checkout = action("checkout");

function money(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

export default function Cart({ lines }: { lines: Line[] }) {
  const total = lines.reduce((sum, l) => sum + Number(l.price_cents) * Number(l.quantity), 0);

  if (lines.length === 0) {
    return (
      <main className="cart">
        <h1>Cart</h1>
        <p className="empty">Nothing in the cart yet.</p>
        <p>
          <a href="/">back to the catalog</a>
        </p>
      </main>
    );
  }

  return (
    <main className="cart">
      <h1>Cart</h1>
      <ul>
        {lines.map((l) => (
          <li key={String(l.id)}>
            <span className="name">{l.name}</span>
            <span className="qty"> x{String(l.quantity)} </span>
            <span className="line">{money(Number(l.price_cents) * Number(l.quantity))}</span>
            <button onClick={() => remove({ product_id: BigInt(l.id), quantity: -1n })}>remove one</button>
          </li>
        ))}
      </ul>
      <p className="total">Total {money(total)}</p>
      <button
        className="checkout"
        onClick={() => {
          void checkout().catch((e) => alert(e.message));
        }}
      >
        checkout
      </button>
      <p>
        <a href="/">back to the catalog</a>
      </p>
    </main>
  );
}
