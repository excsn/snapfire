import { navigate } from "@snapfire/fsr-client";

import { actions, type CartProps } from "@generated/client";
import { confirmOrder, failed, orderPlaced, removedFromCart } from "@src/ui/feedback";
import { Page } from "@src/ui/Page";
import { money } from "@src/ui/money";
import { Thumb } from "@src/ui/Thumb";

export default function Cart({ lines, cartCount }: CartProps) {
  const items = Number(cartCount);
  const subtotal = lines.reduce((sum, l) => sum + Number(l.price_cents) * Number(l.quantity), 0);

  async function change(productId: bigint | number, delta: bigint, name: string): Promise<void> {
    try {
      const result = await actions.cart.addToCart({ product_id: productId, quantity: delta });
      if (!(String(productId) in result.lines)) removedFromCart(name);
    } catch (e) {
      failed(e);
    }
  }

  async function remove(productId: bigint | number, name: string): Promise<void> {
    try {
      await actions.cart.removeFromCart({ product_id: productId });
      removedFromCart(name);
    } catch (e) {
      failed(e);
    }
  }

  async function checkout(): Promise<void> {
    if (!(await confirmOrder(items, subtotal))) return;
    try {
      const order = await actions.cart.checkout();
      orderPlaced(order.id, order.total_cents, order.lines.length);
      void navigate("/");
    } catch (e) {
      failed(e);
    }
  }

  return (
    <Page header={{ cartCount }} className="cart">
      {lines.length === 0 ? (
        <section className="empty">
          <p className="empty-glyph">🛒</p>
          <h1>Your cart is empty</h1>
          <p>Anything you add shows up here, and the count in the header follows.</p>
          <a className="btn btn-primary" href="/">
            Start shopping
          </a>
        </section>
      ) : (
        <div className="cart-layout">
          <section className="cart-lines">
            <h1>Shopping cart</h1>
            <ul>
              {lines.map((l) => {
                const qty = Number(l.quantity);
                const over = qty > Number(l.stock);
                return (
                  <li key={String(l.id)} className="cart-line">
                    <a href={`/product/${String(l.id)}`} className="cart-line-image">
                      <Thumb image={l.image} size="line" />
                    </a>
                    <div className="cart-line-body">
                      <h2>
                        <a href={`/product/${String(l.id)}`}>{l.name}</a>
                      </h2>
                      <p className="card-brand">{l.brand}</p>
                      <p className={over ? "stock stock-out" : "stock stock-in"}>
                        {over ? `Only ${Number(l.stock)} in stock` : "In stock"}
                      </p>
                      <div className="qty">
                        <button aria-label="Remove one" onClick={() => void change(l.id, -1n, l.name)}>
                          −
                        </button>
                        <span>{qty}</span>
                        <button aria-label="Add one" onClick={() => void change(l.id, 1n, l.name)}>
                          +
                        </button>
                        <button className="link" onClick={() => void remove(l.id, l.name)}>
                          Delete
                        </button>
                      </div>
                    </div>
                    <p className="cart-line-price">
                      <span className="price">{money(Number(l.price_cents) * qty)}</span>
                      {qty > 1 ? <span className="unit">{money(l.price_cents)} each</span> : null}
                    </p>
                  </li>
                );
              })}
            </ul>
            <p className="subtotal-line">
              Subtotal ({items} item{items === 1 ? "" : "s"}): <span className="price">{money(subtotal)}</span>
            </p>
          </section>
          <aside className="buy-box">
            <p className="subtotal-line">
              Subtotal ({items} item{items === 1 ? "" : "s"}): <span className="price">{money(subtotal)}</span>
            </p>
            <button className="btn btn-primary btn-block" onClick={() => void checkout()}>
              Proceed to checkout
            </button>
            <a className="btn btn-secondary btn-block" href="/">
              Keep shopping
            </a>
          </aside>
        </div>
      )}
    </Page>
  );
}
