import type { OrderProps } from "@generated/client";
import { Island } from "@snapfire/fsr-client/react";
import { OrderHelp } from "@src/ui/OrderHelp";
import { money } from "@src/ui/money";

export default function OrderPage({ order }: OrderProps) {
  const items = order.lines.reduce((n, l) => n + Number(l.quantity), 0);

  return (
    <main className="page order">
      <section className="order-placed">
        <p className="empty-glyph">📦</p>
        <h1>Order #{String(order.id)} placed</h1>
        <p>
          {items} item{items === 1 ? "" : "s"}, {money(order.total_cents)} charged. Thank you for shopping with us.
        </p>
        <ul className="order-lines">
          {order.lines.map((l) => (
            <li key={String(l.product_id)}>
              <a href={`/product/${String(l.product_id)}`}>{l.name}</a>
              <span className="qty-word">× {Number(l.quantity)}</span>
              <span className="price">{money(l.line_cents)}</span>
            </li>
          ))}
        </ul>
        <a className="btn btn-primary" href="/">
          Back to shopping
        </a>
      </section>
      <Island when="visible">
        <OrderHelp orderId={order.id} />
      </Island>
    </main>
  );
}
