import { actions, type Product } from "@generated/client";
import { categoryLabel } from "./categories";
import { addedToCart, failed } from "./feedback";
import { money, percentOff } from "./money";
import { Stars } from "./Stars";
import { Thumb } from "./Thumb";

export function stockLine(stock: bigint | number): { text: string; className: string } {
  const n = Number(stock);
  if (n === 0) return { text: "Out of stock", className: "stock stock-out" };
  if (n <= 5) return { text: `Only ${n} left`, className: "stock stock-low" };
  return { text: "In stock", className: "stock stock-in" };
}

export function ProductCard({ product }: { product: Product }) {
  const off = percentOff(product.price_cents, product.list_price_cents);
  const stock = stockLine(product.stock);
  const href = `/product/${String(product.id)}`;

  async function add(): Promise<void> {
    try {
      const result = await actions.cart.addToCart({ product_id: product.id, quantity: 1n });
      addedToCart(product.name, result.count);
    } catch (e) {
      failed(e);
    }
  }

  return (
    <article className="card">
      <a href={href} className="card-image">
        <Thumb image={product.image} />
      </a>
      <div className="card-body">
        <p className="card-brand">
          {product.brand} · {categoryLabel(product.category)}
        </p>
        <h2 className="card-title">
          <a href={href}>{product.name}</a>
        </h2>
        <Stars rating={product.rating} reviews={product.reviews} />
        <p className="price-line">
          <span className="price">{money(product.price_cents)}</span>
          {off > 0 && product.list_price_cents != null ? (
            <>
              <span className="list-price">{money(product.list_price_cents)}</span>
              <span className="deal">-{off}%</span>
            </>
          ) : null}
        </p>
        <p className={stock.className}>{stock.text}</p>
        <button className="btn btn-primary" disabled={Number(product.stock) === 0} onClick={() => void add()}>
          Add to cart
        </button>
      </div>
    </article>
  );
}
