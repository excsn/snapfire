import type { OrderProps } from "@generated/client";
import { money } from "@src/ui/money";
import { Page } from "@src/ui/Page";

export default function OrderPage({ order, cartCount }: OrderProps) {
  const items = order.lines.reduce((n, l) => n + Number(l.quantity), 0);

  return (
    <Page header={{ cartCount }} className="order">
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
    </Page>
  );
}
